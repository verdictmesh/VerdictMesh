//! T020 — розрахунок стейків: слешинг і розподіл (`FR-011`, `FR-008b`,
//! `FR-027b`).
//!
//! Задача рахує чужі гроші, і кожна з її помилок тиха.
//!
//! **Слешинг рухає запис, а не токени.** Сховище стейків спільне; списати з
//! нього, не зменшивши `Juror.stake` слешеного, означає дати йому вийти за
//! повною сумою — тобто за чужий стейк, і недорахується останній, хто виходить.
//! Тому тут не переказують нічого: змінюється лише те, скільки кожен може
//! забрати, і сума записів ніколи не перевищує сховище.
//!
//! **Мовчання не буває безкоштовним.** Присяжний, який не подав відбитка
//! взагалі, карається нарівні з тим, хто подав і не розкрився: інакше
//! найдешевшим способом ухилитись від невигідного голосу було б не голосувати
//! зовсім, і `FR-008b` не боронив би нічого.
//!
//! **`active_disputes` знімається рівно на одиницю.** Не обнуляється: присяжний
//! сидить у кількох панелях одночасно, і обнулення випустило б його з реєстру з
//! чужого спору.

#[allow(dead_code)]
#[path = "harness.rs"]
mod harness;

use anchor_lang::solana_program::pubkey::Pubkey;
use harness::*;
use mollusk_svm::result::InstructionResult;
use solana_account::Account;
use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};
use verdict_mesh::{
    events::{DisputeFinalized, JurorRewarded, JurorSlashed},
    state::{Ballot, Dispute, DisputeState, Juror, Policy, Verdict, VoteCommit},
    VerdictMeshError,
};

fn staked() -> u64 {
    usdc(100)
}

/// Що присяжний зробив у розгляді. `Missing` — акаунта голосу немає взагалі:
/// відбитка не подавав.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Did {
    Voted(Ballot),
    Sealed,
    Missing,
}

struct Fixture {
    dispute: Pubkey,
    panel: Vec<Pubkey>,
    accounts: Vec<(Address, Account)>,
    pairs: Vec<Pubkey>,
    policy: Policy,
}

impl Fixture {
    fn new(did: &[Did], verdict: Verdict) -> Self {
        Self::build(did, verdict, DisputeState::Tallied, false)
    }

    fn build(did: &[Did], verdict: Verdict, state: DisputeState, escalated: bool) -> Self {
        let policy = demo_policy();
        let authority = Pubkey::new_unique();
        let (integrator, _) = integrator_pda(&authority);
        let (dispute, bump) = dispute_pda(&integrator, 0);

        let panel: Vec<Pubkey> = did.iter().map(|_| Pubkey::new_unique()).collect();

        let mut tallied = dispute_state(&integrator, 0, &policy, bump);
        tallied.state = state;
        tallied.panel = panel.clone();
        tallied.escalated = escalated;
        tallied.verdict = Some(verdict);
        tallied.appeal_deadline = APPEAL_DEADLINE;

        let mut accounts = vec![(addr(&dispute), dispute_account(&tallied))];
        let mut pairs = Vec::with_capacity(2 * panel.len());

        for (wallet, did) in panel.iter().zip(did) {
            accounts.push((
                addr(&juror_pda(wallet).0),
                program_account(&Juror {
                    wallet: *wallet,
                    stake: staked(),
                    // Двійка, а не одиниця: присяжний сидить іще в чужій панелі,
                    // і розрахунок цього спору не має його звідти випускати.
                    active_disputes: 2,
                    index: 0,
                    bump: juror_pda(wallet).1,
                }),
            ));

            let vote = vote_pda(&dispute, wallet).0;
            accounts.push((
                addr(&vote),
                match did {
                    Did::Missing => missing(),
                    Did::Sealed => vote_account(&dispute, wallet, None),
                    Did::Voted(choice) => vote_account(&dispute, wallet, Some(*choice)),
                },
            ));

            pairs.push(juror_pda(wallet).0);
            pairs.push(vote);
        }

        Self {
            dispute,
            panel,
            accounts,
            pairs,
            policy,
        }
    }

    fn ix(&self) -> Instruction {
        self.ix_with(&self.pairs)
    }

    fn ix_with(&self, pairs: &[Pubkey]) -> Instruction {
        let mut ix = anchor_ix(
            verdict_mesh::accounts::SettleStakes {
                dispute: self.dispute,
            },
            verdict_mesh::instruction::SettleStakes {},
        );

        for key in pairs {
            ix.accounts.push(AccountMeta {
                pubkey: addr(key),
                is_signer: false,
                is_writable: true,
            });
        }

        ix
    }

    fn settle(&self) -> InstructionResult {
        self.settle_at(APPEAL_DEADLINE)
    }

    fn settle_at(&self, now: i64) -> InstructionResult {
        mollusk_at(now).process_instruction(&self.ix(), &self.accounts)
    }

    fn ok(&self) -> InstructionResult {
        let result = self.settle();
        assert!(result.program_result.is_ok(), "{:?}", result.raw_result);
        result
    }

    fn stake_of(&self, result: &InstructionResult, index: usize) -> u64 {
        let juror: Juror = decode(resulting(result, &juror_pda(&self.panel[index]).0));
        juror.stake
    }

    fn locks_of(&self, result: &InstructionResult, index: usize) -> u16 {
        let juror: Juror = decode(resulting(result, &juror_pda(&self.panel[index]).0));
        juror.active_disputes
    }

    fn slash(&self, bps: u16) -> u64 {
        staked() * u64::from(bps) / 10_000
    }
}

/// Вікно апеляції закривається тут — раніше розраховувати нічого не можна.
const APPEAL_DEADLINE: i64 = NOW + 10_000;

fn vote_account(dispute: &Pubkey, juror: &Pubkey, choice: Option<Ballot>) -> Account {
    program_account(&VoteCommit {
        dispute: *dispute,
        juror: *juror,
        commitment: [9u8; 32],
        choice,
        round: 0,
        bump: vote_pda(dispute, juror).1,
    })
}

// ── слешинг ─────────────────────────────────────────────────────────────────

/// `FR-011`: чий голос не збігся з підсумковим, той втрачає частку стейку.
#[test]
fn slashes_the_juror_whose_vote_lost() {
    let fixture = Fixture::new(
        &[
            Did::Voted(Ballot::Claimant),
            Did::Voted(Ballot::Claimant),
            Did::Voted(Ballot::Respondent),
        ],
        Verdict::Claimant,
    );
    let result = fixture.ok();

    assert_eq!(
        fixture.stake_of(&result, 2),
        staked() - fixture.slash(fixture.policy.slash_bps_wrong)
    );
}

/// `FR-008b`: подав відбиток і не розкрився — втрачає більшу частку, ніж той,
/// хто просто програв. Інакше мовчання було б найдешевшим способом не програти.
#[test]
fn slashes_silence_harder_than_a_lost_vote() {
    let fixture = Fixture::new(
        &[
            Did::Voted(Ballot::Claimant),
            Did::Voted(Ballot::Respondent),
            Did::Sealed,
        ],
        Verdict::Claimant,
    );
    let result = fixture.ok();

    let lost = staked() - fixture.stake_of(&result, 1);
    let silent = staked() - fixture.stake_of(&result, 2);
    assert_eq!(lost, fixture.slash(fixture.policy.slash_bps_wrong));
    assert_eq!(silent, fixture.slash(fixture.policy.slash_bps_no_reveal));
    assert!(silent > lost);
}

/// Найважливіший тест файлу. Присяжний, який не подав відбитка **взагалі**,
/// карається нарівні з тим, хто подав і змовчав. `FR-008b` говорить лише про
/// другого, і буквальне читання лишило б безкоштовним найдешевший спосіб
/// ухилитись від невигідного голосу — не голосувати зовсім.
#[test]
fn slashes_a_juror_who_never_even_sealed_a_vote() {
    let fixture = Fixture::new(
        &[
            Did::Voted(Ballot::Claimant),
            Did::Voted(Ballot::Claimant),
            Did::Missing,
        ],
        Verdict::Claimant,
    );
    let result = fixture.ok();

    assert_eq!(
        fixture.stake_of(&result, 2),
        staked() - fixture.slash(fixture.policy.slash_bps_no_reveal)
    );
}

/// Той, чий голос збігся, стейку не втрачає.
#[test]
fn leaves_the_stake_of_a_juror_who_was_right() {
    let fixture = Fixture::new(
        &[
            Did::Voted(Ballot::Claimant),
            Did::Voted(Ballot::Claimant),
            Did::Voted(Ballot::Respondent),
        ],
        Verdict::Claimant,
    );
    let result = fixture.ok();

    assert!(fixture.stake_of(&result, 0) >= staked());
}

/// Статус-кво — не «всі програли». Ніхто не голосував за нього і голосувати не
/// міг: це наслідок невдалої ескалації, а не бюлетень (`FR-027a`). Карати за
/// нього тих, хто чесно розкрився, означало б карати за те, що панель не
/// зійшлась.
#[test]
fn slashes_nobody_for_a_status_quo_they_could_not_vote_for() {
    let fixture = Fixture::build(
        &[
            Did::Voted(Ballot::Claimant),
            Did::Voted(Ballot::Respondent),
            Did::Sealed,
        ],
        Verdict::StatusQuo,
        DisputeState::Tallied,
        true,
    );
    let result = fixture.ok();

    assert!(fixture.stake_of(&result, 0) >= staked());
    assert!(fixture.stake_of(&result, 1) >= staked());
    assert_eq!(
        fixture.stake_of(&result, 2),
        staked() - fixture.slash(fixture.policy.slash_bps_no_reveal),
        "мовчання карається і за статус-кво — FR-027b"
    );
}

// ── розподіл ────────────────────────────────────────────────────────────────

/// Слешене не зникає і не осідає мертвим залишком: воно дістається тим, чий
/// голос збігся. Без цього правильний голос не приносить нічого, а сховище
/// накопичує суму, яку ніхто не може забрати.
#[test]
fn hands_what_was_slashed_to_the_jurors_who_were_right() {
    let fixture = Fixture::new(
        &[
            Did::Voted(Ballot::Claimant),
            Did::Voted(Ballot::Claimant),
            Did::Voted(Ballot::Respondent),
        ],
        Verdict::Claimant,
    );
    let result = fixture.ok();

    let pot = fixture.slash(fixture.policy.slash_bps_wrong);
    let gained = fixture.stake_of(&result, 0) - staked() + fixture.stake_of(&result, 1) - staked();
    assert_eq!(gained, pot);
}

/// Сума записів після розрахунку не перевищує тієї, що була: сховище спільне, і
/// розподіл, який створює токени з нічого, вивів би чужі стейки.
#[test]
fn never_hands_out_more_than_it_took() {
    for verdict in [Verdict::Claimant, Verdict::Respondent, Verdict::StatusQuo] {
        let fixture = Fixture::build(
            &[
                Did::Voted(Ballot::Claimant),
                Did::Voted(Ballot::Respondent),
                Did::Sealed,
            ],
            verdict,
            DisputeState::Tallied,
            true,
        );
        let result = fixture.ok();

        let total: u64 = (0..3).map(|index| fixture.stake_of(&result, index)).sum();
        assert!(total <= 3 * staked(), "{verdict:?} створив стейк із нічого");
    }
}

/// Залишок від ділення не губиться. Дрібниця, яка за сотні розглядів стає
/// сумою, що не належить нікому і не сходиться зі сховищем.
#[test]
fn leaves_no_dust_behind_when_the_pot_does_not_divide() {
    let fixture = Fixture::new(
        &[
            Did::Voted(Ballot::Claimant),
            Did::Voted(Ballot::Claimant),
            Did::Voted(Ballot::Claimant),
            Did::Voted(Ballot::Respondent),
        ],
        Verdict::Claimant,
    );
    let result = fixture.ok();

    let pot = fixture.slash(fixture.policy.slash_bps_wrong);
    let handed: u64 = (0..3)
        .map(|index| fixture.stake_of(&result, index) - staked())
        .sum();
    assert_eq!(handed, pot);
}

/// Розкриті голоси за статус-кво не «правильні», але й не програні — саме вони
/// і є тими, хто зробив роботу. Слешене за мовчання дістається їм.
#[test]
fn hands_a_status_quo_pot_to_those_who_actually_revealed() {
    let fixture = Fixture::build(
        &[
            Did::Voted(Ballot::Claimant),
            Did::Voted(Ballot::Respondent),
            Did::Sealed,
        ],
        Verdict::StatusQuo,
        DisputeState::Tallied,
        true,
    );
    let result = fixture.ok();

    let pot = fixture.slash(fixture.policy.slash_bps_no_reveal);
    let handed = fixture.stake_of(&result, 0) - staked() + fixture.stake_of(&result, 1) - staked();
    assert_eq!(handed, pot);
}

/// Нікому платити — і це не привід зупинити розрахунок. Панель, що змовчала
/// цілком, мусить розрахуватись, інакше `active_disputes` не знімається і всі
/// троє лишаються в реєстрі назавжди.
#[test]
fn settles_a_panel_that_left_nobody_to_pay() {
    let fixture = Fixture::new(
        &[Did::Sealed, Did::Missing, Did::Sealed],
        Verdict::StatusQuo,
    );
    let result = fixture.ok();

    for index in 0..3 {
        assert_eq!(
            fixture.stake_of(&result, index),
            staked() - fixture.slash(fixture.policy.slash_bps_no_reveal)
        );
        assert_eq!(fixture.locks_of(&result, index), 1);
    }
}

// ── реєстр ──────────────────────────────────────────────────────────────────

/// Те, заради чого відбір узагалі піднімав лічильник. Не знявши його, кожен,
/// хто хоч раз потрапив у панель, лишається в реєстрі назавжди — і побачити це
/// можна аж у `unstake`.
#[test]
fn releases_every_juror_from_this_dispute() {
    let fixture = Fixture::new(
        &[
            Did::Voted(Ballot::Claimant),
            Did::Voted(Ballot::Claimant),
            Did::Sealed,
        ],
        Verdict::Claimant,
    );
    let result = fixture.ok();

    for index in 0..3 {
        assert_eq!(
            fixture.locks_of(&result, index),
            1,
            "лічильник обнулили замість зняти одиницю"
        );
    }
}

// ── межі ────────────────────────────────────────────────────────────────────

/// Розраховувати до кінця вікна апеляції означає розрахувати вердикт, який ще
/// можуть перекрити.
#[test]
fn refuses_a_settlement_before_the_appeal_window_closes() {
    let fixture = Fixture::new(&[Did::Voted(Ballot::Claimant); 3], Verdict::Claimant);
    let result = fixture.settle_at(APPEAL_DEADLINE - 1);
    assert!(failed_with(&result, VerdictMeshError::WindowOpen));
}

/// Розрахунок одноразовий. Другий прогін злив би стейк іще раз і зняв би
/// лічильник за спір, у якому присяжний уже не бере участі.
#[test]
fn refuses_to_settle_the_same_dispute_twice() {
    let fixture = Fixture::new(&[Did::Voted(Ballot::Claimant); 3], Verdict::Claimant);
    let first = fixture.ok();

    let mut accounts = fixture.accounts.clone();
    replace(
        &mut accounts,
        &fixture.dispute,
        resulting(&first, &fixture.dispute).clone(),
    );

    let result = mollusk_at(APPEAL_DEADLINE).process_instruction(&fixture.ix(), &accounts);
    assert!(failed_with(&result, VerdictMeshError::WrongState));
}

#[test]
fn refuses_a_dispute_that_has_no_verdict_yet() {
    for state in [
        DisputeState::Committing,
        DisputeState::Revealing,
        DisputeState::Appealed,
        DisputeState::Finalized,
    ] {
        let fixture = Fixture::build(
            &[Did::Voted(Ballot::Claimant); 3],
            Verdict::Claimant,
            state,
            false,
        );
        let result = fixture.settle();
        assert!(
            failed_with(&result, VerdictMeshError::WrongState),
            "{state:?} accepted a settlement"
        );
    }
}

/// Розрахунок закриває спір. Доти нічого не заважає викликати його ще раз, і
/// саме стан — те, що робить одноразовість перевірюваною.
#[test]
fn finalizes_the_dispute() {
    let fixture = Fixture::new(&[Did::Voted(Ballot::Claimant); 3], Verdict::Claimant);
    let result = fixture.ok();

    let dispute: Dispute = decode(resulting(&result, &fixture.dispute));
    assert_eq!(dispute.state, DisputeState::Finalized);
}

/// Панель передається цілком і в тому ж порядку. Показати розрахунку лише
/// зручних присяжних означало б не слешити решту й лишити їх у реєстрі.
#[test]
fn refuses_a_panel_that_is_not_enumerated_in_full() {
    let fixture = Fixture::new(&[Did::Voted(Ballot::Claimant); 3], Verdict::Claimant);

    let short = fixture.pairs[..4].to_vec();
    let result = mollusk_at(APPEAL_DEADLINE)
        .process_instruction(&fixture.ix_with(&short), &fixture.accounts);
    assert!(failed_with(
        &result,
        VerdictMeshError::InvalidSettlementAccounts
    ));
}

/// Запис присяжного звіряється з панеллю за адресою. Підставлений `Juror`
/// прийняв би слешинг замість того, кому він призначений.
#[test]
fn refuses_a_juror_record_that_is_not_on_the_panel() {
    let fixture = Fixture::new(&[Did::Voted(Ballot::Claimant); 3], Verdict::Claimant);
    let stranger = Pubkey::new_unique();

    let mut pairs = fixture.pairs.clone();
    pairs[0] = juror_pda(&stranger).0;

    let mut accounts = fixture.accounts.clone();
    accounts.push((
        addr(&juror_pda(&stranger).0),
        program_account(&Juror {
            wallet: stranger,
            stake: staked(),
            active_disputes: 1,
            index: 0,
            bump: juror_pda(&stranger).1,
        }),
    ));

    let result =
        mollusk_at(APPEAL_DEADLINE).process_instruction(&fixture.ix_with(&pairs), &accounts);
    assert!(failed_with(
        &result,
        VerdictMeshError::InvalidSettlementAccounts
    ));
}

/// Чужий акаунт голосу — той самий випадок: він показав би розрахунку голос,
/// якого цей присяжний не подавав.
#[test]
fn refuses_a_vote_account_belonging_to_another_juror() {
    let fixture = Fixture::new(
        &[
            Did::Voted(Ballot::Claimant),
            Did::Voted(Ballot::Respondent),
            Did::Voted(Ballot::Claimant),
        ],
        Verdict::Claimant,
    );

    let mut pairs = fixture.pairs.clone();
    pairs[3] = vote_pda(&fixture.dispute, &fixture.panel[0]).0;

    let result = mollusk_at(APPEAL_DEADLINE)
        .process_instruction(&fixture.ix_with(&pairs), &fixture.accounts);
    assert!(failed_with(
        &result,
        VerdictMeshError::InvalidSettlementAccounts
    ));
}

// ── події ───────────────────────────────────────────────────────────────────

/// `FR-029`: слешинг видно ззовні поіменно й із причиною — інакше присяжний
/// бачить меншу суму і не бачить, за що.
#[test]
fn announces_every_slash_with_its_reason() {
    let fixture = Fixture::new(
        &[
            Did::Voted(Ballot::Claimant),
            Did::Voted(Ballot::Respondent),
            Did::Sealed,
        ],
        Verdict::Claimant,
    );

    let (mut mollusk, logs) = mollusk_with_logs();
    mollusk.sysvars.clock.unix_timestamp = APPEAL_DEADLINE;
    let result = mollusk.process_instruction(&fixture.ix(), &fixture.accounts);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let events: Vec<JurorSlashed> = emitted(&logs);
    assert_eq!(events.len(), 2);

    let lost = events
        .iter()
        .find(|event| event.juror == fixture.panel[1])
        .expect("програний голос слешено");
    assert!(!lost.no_reveal);
    assert_eq!(lost.amount, fixture.slash(fixture.policy.slash_bps_wrong));

    let silent = events
        .iter()
        .find(|event| event.juror == fixture.panel[2])
        .expect("мовчання слешено");
    assert!(silent.no_reveal);
    assert_eq!(
        silent.amount,
        fixture.slash(fixture.policy.slash_bps_no_reveal)
    );
}

#[test]
fn announces_the_reward_and_the_finalization() {
    let fixture = Fixture::new(
        &[
            Did::Voted(Ballot::Claimant),
            Did::Voted(Ballot::Claimant),
            Did::Voted(Ballot::Respondent),
        ],
        Verdict::Claimant,
    );

    let (mut mollusk, logs) = mollusk_with_logs();
    mollusk.sysvars.clock.unix_timestamp = APPEAL_DEADLINE;
    let result = mollusk.process_instruction(&fixture.ix(), &fixture.accounts);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let rewards: Vec<JurorRewarded> = emitted(&logs);
    assert_eq!(rewards.len(), 2);
    assert_eq!(
        rewards.iter().map(|event| event.amount).sum::<u64>(),
        fixture.slash(fixture.policy.slash_bps_wrong)
    );

    let finalized: Vec<DisputeFinalized> = emitted(&logs);
    assert_eq!(finalized.len(), 1);
    assert_eq!(finalized[0].verdict, Verdict::Claimant);
    assert_eq!(finalized[0].finalized_at, APPEAL_DEADLINE);
}
