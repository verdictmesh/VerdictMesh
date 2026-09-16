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
    events::{DisputeFeeSettled, DisputeFinalized, JurorRewarded, JurorSlashed},
    state::{Ballot, Dispute, DisputeState, Juror, Policy, Verdict, VoteCommit},
    vault::JUROR_FEE_BPS,
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
    mint: Pubkey,
    crank: Pubkey,
    treasury_tokens: Pubkey,
    dispute_vault: Pubkey,
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

        let mint = Pubkey::new_unique();
        let crank = Pubkey::new_unique();
        let treasury = Pubkey::new_unique();
        let treasury_tokens = Pubkey::new_unique();
        let dispute_vault = dispute_vault_pda(&dispute).0;

        let mut accounts = vec![
            (addr(&dispute), dispute_account(&tallied)),
            (addr(&crank), wallet(1_000_000_000)),
            (
                addr(&config_pda().0),
                config_account(&mint, &Pubkey::new_unique(), &treasury),
            ),
            (addr(&mint), settlement_mint()),
            // Сховище спору тримає рівно депозит — те, що поклав туди
            // `open_dispute`. Порожнє сховище було б станом, якого на ланцюгу
            // не буває, і тести розподілу перевіряли б неіснуючий випадок.
            (addr(&dispute_vault), vault_account(&mint, policy.deposit)),
            (
                addr(&stake_vault_pda().0),
                vault_account(&mint, staked() * panel.len() as u64),
            ),
            (addr(&treasury_tokens), token_account(&mint, &treasury, 0)),
            keyed_account_for_token_program(),
        ];
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
            mint,
            crank,
            treasury_tokens,
            dispute_vault,
        }
    }

    /// Сховище спору з іншим балансом. Потрібне рівно там, де перевіряється
    /// нестача (`FR-026c`): решта тестів мусить бачити депозит, а не число,
    /// підібране під очікування.
    fn holding(mut self, amount: u64) -> Self {
        let account = vault_account(&self.mint, amount);
        replace(&mut self.accounts, &self.dispute_vault, account);
        self
    }

    fn ix(&self) -> Instruction {
        self.ix_with(&self.pairs)
    }

    fn ix_with(&self, pairs: &[Pubkey]) -> Instruction {
        let mut ix = anchor_ix(
            verdict_mesh::accounts::SettleStakes {
                dispute: self.dispute,
                crank: self.crank,
                config: config_pda().0,
                settlement_mint: self.mint,
                dispute_vault: self.dispute_vault,
                stake_vault: stake_vault_pda().0,
                treasury_tokens: self.treasury_tokens,
                token_program: TOKEN_PROGRAM,
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

    /// Частка присяжних в оплаті розгляду — `FR-026b`.
    fn fee_to_jurors(&self) -> u64 {
        self.policy.deposit * u64::from(JUROR_FEE_BPS) / 10_000
    }

    fn fee_to_protocol(&self) -> u64 {
        self.policy.deposit - self.fee_to_jurors()
    }

    fn balance(&self, result: &InstructionResult, key: &Pubkey) -> u64 {
        token_state(resulting(result, key)).amount
    }

    /// Скільки всього записано за присяжними панелі. Число, яке має ходити в
    /// крок зі сховищем стейків.
    fn recorded(&self, result: &InstructionResult) -> u64 {
        (0..self.panel.len())
            .map(|index| self.stake_of(result, index))
            .sum()
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

    let pot = fixture.slash(fixture.policy.slash_bps_wrong) + fixture.fee_to_jurors();
    let gained = fixture.stake_of(&result, 0) - staked() + fixture.stake_of(&result, 1) - staked();
    assert_eq!(gained, pot);
}

/// Найважливіша рівність файлу: сума записів росте рівно на стільки, на скільки
/// приросло сховище стейків. Слешинг лише пересуває записи, комісія доливає
/// токени — і будь-яка інша арифметика означала б, що останній, хто виходить із
/// реєстру, недорахується чужого стейку.
#[test]
fn keeps_the_records_in_step_with_the_stake_vault() {
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
        let before = staked() * 3;
        let result = fixture.ok();

        let recorded = fixture.recorded(&result);
        let vault = fixture.balance(&result, &stake_vault_pda().0);

        // Приросло рівно на комісію — і в сховищі, і в записах. Слешинг у цій
        // сумі не видно взагалі, і саме так і має бути: він нічого не створює
        // і нічого не знищує, лише переставляє між своїми.
        assert_eq!(
            vault,
            before + fixture.fee_to_jurors(),
            "{verdict:?}: у сховищі стейків не те, що переказали"
        );
        assert_eq!(
            recorded,
            before + fixture.fee_to_jurors(),
            "{verdict:?}: сума записів розійшлася зі сховищем"
        );
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

    let pot = fixture.slash(fixture.policy.slash_bps_wrong) + fixture.fee_to_jurors();
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

    let pot = fixture.slash(fixture.policy.slash_bps_no_reveal) + fixture.fee_to_jurors();
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

// ── FR-026b: оплата розгляду ────────────────────────────────────────────────

/// Депозит розходиться повністю: більша частина — присяжним, залишок —
/// протоколу, а сховище спору закривається порожнім. Сума, що лишилась би в
/// ньому, не належала б нікому: інструкції, здатної її дістати, у програмі
/// немає (`FR-014`).
#[test]
fn splits_the_deposit_between_the_jurors_and_the_protocol() {
    let fixture = Fixture::new(
        &[
            Did::Voted(Ballot::Claimant),
            Did::Voted(Ballot::Claimant),
            Did::Voted(Ballot::Respondent),
        ],
        Verdict::Claimant,
    );
    let before = staked() * 3;
    let result = fixture.ok();

    assert_eq!(
        fixture.balance(&result, &stake_vault_pda().0),
        before + fixture.fee_to_jurors()
    );
    assert_eq!(
        fixture.balance(&result, &fixture.treasury_tokens),
        fixture.fee_to_protocol()
    );
}

/// Комісія приходить присяжному тим самим записом, що й злетіле зі стейків, —
/// але під неї треба справді перевести токени. Тест дивиться на обидва боки
/// одразу: у переможця більше на свою частку, і рівно на неї ж більше в
/// сховищі стейків.
#[test]
fn pays_the_review_to_those_whose_vote_matched() {
    let fixture = Fixture::new(
        &[
            Did::Voted(Ballot::Claimant),
            Did::Voted(Ballot::Respondent),
            Did::Voted(Ballot::Respondent),
        ],
        Verdict::Respondent,
    );
    let result = fixture.ok();

    let pot = fixture.slash(fixture.policy.slash_bps_wrong) + fixture.fee_to_jurors();
    for index in [1, 2] {
        assert_eq!(fixture.stake_of(&result, index), staked() + pot / 2);
    }
    assert_eq!(
        fixture.stake_of(&result, 0),
        staked() - fixture.slash(fixture.policy.slash_bps_wrong)
    );
}

/// Статус-кво не залишає панель без оплати. Збігатися з вердиктом там немає з
/// чим, але розгляд відбувся, і той, хто розкрився, зробив ту саму роботу —
/// це те саме рішення, що й у розподілі злетілого.
#[test]
fn pays_the_review_even_when_the_panel_ended_in_a_status_quo() {
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
    let before = staked() * 3;
    let result = fixture.ok();

    assert_eq!(
        fixture.balance(&result, &stake_vault_pda().0),
        before + fixture.fee_to_jurors()
    );
}

/// Панель змовчала цілком — платити нікому, і частка присяжних не має де
/// осісти. Вона йде протоколу, а не лишається у сховищі, яке закривається: там
/// вона просто зникла б.
#[test]
fn hands_the_whole_review_to_the_protocol_when_nobody_was_right() {
    let fixture = Fixture::new(
        &[Did::Sealed, Did::Missing, Did::Sealed],
        Verdict::StatusQuo,
    );
    let before = staked() * 3;
    let result = fixture.ok();

    assert_eq!(
        fixture.balance(&result, &fixture.treasury_tokens),
        fixture.policy.deposit
    );
    assert_eq!(fixture.balance(&result, &stake_vault_pda().0), before);
}

/// `FR-026c`: недобір падає на протокол, а не на присяжного. Присяжний рахує
/// свій заробіток наперед з оголошеної ціни розгляду і не має як перевірити,
/// скільки дійшло до сховища; протокол має.
#[test]
fn lets_the_protocol_go_short_before_a_juror_does() {
    let short = usdc(4);
    let fixture = Fixture::new(
        &[
            Did::Voted(Ballot::Claimant),
            Did::Voted(Ballot::Claimant),
            Did::Voted(Ballot::Respondent),
        ],
        Verdict::Claimant,
    )
    .holding(short);
    let before = staked() * 3;
    let result = fixture.ok();

    assert_eq!(short, fixture.fee_to_jurors());
    assert_eq!(
        fixture.balance(&result, &stake_vault_pda().0),
        before + fixture.fee_to_jurors()
    );
    assert_eq!(fixture.balance(&result, &fixture.treasury_tokens), 0);
}

/// Сховище спору закривається завжди — інакше на кожен розгляд лишався б
/// порожній токен-акаунт із замкненою орендою, і накопичувалась би вона рівно з
/// тією швидкістю, з якою протокол працює. Оренда дістається кранку: це і є
/// причина взагалі викликати дозвільну інструкцію.
#[test]
fn closes_the_dispute_vault_and_pays_the_crank_its_rent() {
    let fixture = Fixture::new(&[Did::Voted(Ballot::Claimant); 3], Verdict::Claimant);
    let result = fixture.ok();

    // Закритий акаунт — це нуль лампортів і стерті дані; розпакувати з нього
    // баланс уже неможливо, і саме це й означає «закритий».
    let vault = resulting(&result, &fixture.dispute_vault);
    assert_eq!(vault.lamports, 0, "сховище спору лишилось відкритим");
    assert!(
        vault.data.iter().all(|byte| *byte == 0),
        "у закритому сховищі лишились дані"
    );
    assert!(
        resulting(&result, &fixture.crank).lamports > 1_000_000_000,
        "оренда сховища не дійшла до кранка"
    );
}

/// Скарбниця перевіряється за власником із `Config`, а не «якийсь токен-акаунт
/// того самого мінта». Без цього комісію протоколу забирав би той, хто першим
/// викличе кранк зі своїм акаунтом.
#[test]
fn refuses_a_treasury_account_owned_by_somebody_else() {
    let fixture = Fixture::new(&[Did::Voted(Ballot::Claimant); 3], Verdict::Claimant);

    let mut accounts = fixture.accounts.clone();
    replace(
        &mut accounts,
        &fixture.treasury_tokens,
        token_account(&fixture.mint, &Pubkey::new_unique(), 0),
    );

    let result = mollusk_at(APPEAL_DEADLINE).process_instruction(&fixture.ix(), &accounts);
    assert!(result.program_result.is_err());
}

/// `FR-029`: куди розійшлась оплата, видно з подій. Двох чисел досить, щоб
/// звести баланс сховища, якого після розрахунку вже не існує.
#[test]
fn announces_where_the_review_payment_went() {
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

    let events: Vec<DisputeFeeSettled> = emitted(&logs);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].dispute, fixture.dispute);
    assert_eq!(events[0].jurors, fixture.fee_to_jurors());
    assert_eq!(events[0].protocol, fixture.fee_to_protocol());
    assert_eq!(
        events[0].jurors + events[0].protocol,
        fixture.policy.deposit
    );
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

    // Подія несе те, що присяжний справді отримав, — злетіле й комісію разом.
    // Розділяти їх у події означало б обіцяти спостерігачу два джерела там, де
    // виплата одна: `DisputeFeeSettled` уже каже, скільки з цього — оплата.
    let rewards: Vec<JurorRewarded> = emitted(&logs);
    assert_eq!(rewards.len(), 2);
    assert_eq!(
        rewards.iter().map(|event| event.amount).sum::<u64>(),
        fixture.slash(fixture.policy.slash_bps_wrong) + fixture.fee_to_jurors()
    );

    let finalized: Vec<DisputeFinalized> = emitted(&logs);
    assert_eq!(finalized.len(), 1);
    assert_eq!(finalized[0].verdict, Verdict::Claimant);
    assert_eq!(finalized[0].finalized_at, APPEAL_DEADLINE);
}
