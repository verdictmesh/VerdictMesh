//! T023 — виконання вердикту в ескроу (`FR-012`, `FR-013`).
//!
//! **Вердикт витягують, а не проштовхують.** Ескроу читає акаунт `Dispute` як
//! звичайний ончейн-стан і розподіляє кошти сам. VerdictMesh не викликає сюди
//! нічого і не має тут жодного повноваження — тому й тести побудовані так, що
//! спір у них просто **лежить у фікстурі**: жодної інструкції арбітражу для
//! виплати не потрібно.
//!
//! **Перевірок чотири, а не три.** Три очевидні — спір наш, вердикт є, вікно
//! апеляції минуло. Четверта менш очевидна і найважливіша: адреса розгляду має
//! збігтися з тією, що лежить **у стані віхи**. Без неї будь-який інший спір
//! цієї ж угоди — навіть давно виконаний — зійшовся б за `escrow_ref` і
//! розпорядився б віхою, якої не стосувався. Саме вона робить `FR-013`
//! властивістю конструкції.

#[allow(dead_code)]
#[path = "harness.rs"]
mod harness;

use anchor_lang::solana_program::pubkey::Pubkey;
use harness::*;
use mollusk_svm::result::InstructionResult;
use reference_escrow::{
    events::MilestoneSettled,
    state::{Escrow, MilestoneState},
    EscrowError,
};
use solana_account::Account;
use solana_address::Address;
use solana_instruction::Instruction;
use verdict_mesh::state::{DisputeState, Verdict};

const DEAL: u64 = 7;
const DISPUTED: u8 = 1;

fn milestones() -> Vec<u64> {
    vec![usdc(10), usdc(20), usdc(30)]
}

fn total() -> u64 {
    milestones().iter().sum()
}

fn disputed_amount() -> u64 {
    milestones()[DISPUTED as usize]
}

/// Угода, у якій одна віха вже під розглядом, і сам розгляд. Обидва акаунти
/// викладені фікстурою, а не отримані прогоном попередніх інструкцій: виплата
/// має перевірятись сама по собі, а не разом з усім, що до неї привело.
struct Fixture {
    buyer: Pubkey,
    seller: Pubkey,
    mint: Pubkey,
    escrow: Pubkey,
    vault: Pubkey,
    buyer_tokens: Pubkey,
    seller_tokens: Pubkey,
    integrator: Pubkey,
    dispute: Pubkey,
    accounts: Vec<(Address, Account)>,
}

impl Fixture {
    /// Спір, який відкрив виконавець і виграв: вердикт на боці ініціатора.
    fn new() -> Self {
        Self::build(true, Some(Verdict::Claimant), DisputeState::Tallied)
    }

    fn build(by_seller: bool, verdict: Option<Verdict>, state: DisputeState) -> Self {
        let buyer = Pubkey::new_unique();
        let seller = Pubkey::new_unique();
        let authority = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        let buyer_tokens = Pubkey::new_unique();
        let seller_tokens = Pubkey::new_unique();

        let (integrator, _) = mesh_integrator_pda(&authority);
        let (escrow, escrow_bump) = escrow_pda(&buyer, DEAL);
        let (vault, _) = escrow_vault_pda(&escrow);
        let (dispute, _) = mesh_dispute_pda(&integrator, 0);

        let (claimant, respondent) = if by_seller {
            (seller, buyer)
        } else {
            (buyer, seller)
        };

        let mut hearing = mesh_dispute_state(
            &integrator,
            0,
            &escrow,
            &claimant,
            &respondent,
            disputed_amount(),
        );
        hearing.state = state;
        hearing.verdict = verdict;

        let accounts = vec![
            (
                addr(&escrow),
                escrow_account(&buyer, &seller, &mint, &integrator, escrow_bump, &dispute),
            ),
            (addr(&dispute), mesh_dispute_account(&hearing)),
            (addr(&mint), spl_mint(DECIMALS as u8)),
            (addr(&buyer_tokens), token_account(&mint, &buyer, 0)),
            (addr(&seller_tokens), token_account(&mint, &seller, 0)),
            (addr(&vault), token_account(&mint, &escrow, total())),
            keyed_account_for_token_program(),
        ];

        Self {
            buyer,
            seller,
            mint,
            escrow,
            vault,
            buyer_tokens,
            seller_tokens,
            integrator,
            dispute,
            accounts,
        }
    }

    fn ix(&self, milestone: u8) -> Instruction {
        self.ix_over(self.dispute, milestone)
    }

    fn ix_over(&self, dispute: Pubkey, milestone: u8) -> Instruction {
        self.ix_paying(dispute, milestone, self.buyer_tokens, self.seller_tokens)
    }

    fn ix_paying(
        &self,
        dispute: Pubkey,
        milestone: u8,
        buyer_tokens: Pubkey,
        seller_tokens: Pubkey,
    ) -> Instruction {
        anchor_ix(
            &ESCROW_PROGRAM,
            reference_escrow::accounts::SettleMilestone {
                escrow: self.escrow,
                dispute,
                mint: self.mint,
                buyer_tokens,
                seller_tokens,
                vault: self.vault,
                token_program: TOKEN_PROGRAM,
            },
            reference_escrow::instruction::SettleMilestone { milestone },
        )
    }

    fn settle(&self) -> InstructionResult {
        mollusk_at(APPEAL_DEADLINE).process_instruction(&self.ix(DISPUTED), &self.accounts)
    }

    fn ok(&self) -> InstructionResult {
        let result = self.settle();
        assert!(result.program_result.is_ok(), "{:?}", result.raw_result);
        result
    }

    fn escrow_state(&self, result: &InstructionResult) -> Escrow {
        decode(resulting(result, &self.escrow))
    }

    fn balance(&self, result: &InstructionResult, key: &Pubkey) -> u64 {
        token_state(resulting(result, key)).amount
    }
}

/// Угода, у якій віха `DISPUTED` уже під названим розглядом. Це те, що лишає по
/// собі `dispute_milestone` (T022).
fn escrow_account(
    buyer: &Pubkey,
    seller: &Pubkey,
    mint: &Pubkey,
    integrator: &Pubkey,
    bump: u8,
    dispute: &Pubkey,
) -> Account {
    let milestones = milestones()
        .into_iter()
        .enumerate()
        .map(|(index, amount)| reference_escrow::state::Milestone {
            amount,
            state: if index == DISPUTED as usize {
                MilestoneState::Disputed { dispute: *dispute }
            } else {
                MilestoneState::Pending
            },
        })
        .collect::<Vec<_>>();

    let count = milestones.len();
    let mut account = program_account(
        &ESCROW_PROGRAM,
        &Escrow {
            buyer: *buyer,
            seller: *seller,
            mint: *mint,
            integrator: *integrator,
            deal_id: DEAL,
            milestones,
            bump,
        },
    );
    account.data.resize(Escrow::space(count), 0);
    account
}

// ── розподіл ────────────────────────────────────────────────────────────────

/// Ініціатор-виконавець виграв: віха йде йому. `Verdict::Claimant` означає
/// «позиція ініціатора перемогла», а позиція виводиться з ролі (T022).
#[test]
fn pays_the_milestone_to_the_seller_who_won_his_own_dispute() {
    let fixture = Fixture::new();
    let result = fixture.ok();

    assert_eq!(
        fixture.balance(&result, &fixture.seller_tokens),
        disputed_amount()
    );
    assert_eq!(fixture.balance(&result, &fixture.buyer_tokens), 0);
    assert_eq!(
        fixture.balance(&result, &fixture.vault),
        total() - disputed_amount()
    );
    assert_eq!(
        fixture.escrow_state(&result).milestones[DISPUTED as usize].state,
        MilestoneState::Released
    );
}

/// Той самий вердикт із протилежним ініціатором дає протилежний рух коштів.
/// Тест саме про це: сторона вердикту нічого не каже про напрямок виплати, доки
/// не спитати, хто відкривав спір.
#[test]
fn refunds_the_milestone_to_the_buyer_who_won_his_own_dispute() {
    let fixture = Fixture::build(false, Some(Verdict::Claimant), DisputeState::Tallied);
    let result = fixture.ok();

    assert_eq!(
        fixture.balance(&result, &fixture.buyer_tokens),
        disputed_amount()
    );
    assert_eq!(fixture.balance(&result, &fixture.seller_tokens), 0);
    assert_eq!(
        fixture.escrow_state(&result).milestones[DISPUTED as usize].state,
        MilestoneState::Refunded
    );
}

/// Вердикт на боці відповідача — теж вердикт. Спір відкрив замовник і програв,
/// тож віха йде виконавцю.
#[test]
fn honours_a_verdict_that_went_to_the_respondent() {
    let fixture = Fixture::build(false, Some(Verdict::Respondent), DisputeState::Tallied);
    let result = fixture.ok();

    assert_eq!(
        fixture.balance(&result, &fixture.seller_tokens),
        disputed_amount()
    );
    assert_eq!(
        fixture.escrow_state(&result).milestones[DISPUTED as usize].state,
        MilestoneState::Released
    );
}

/// `FR-027a`: статус-кво — «як ніби спору не було». Віха повертається в
/// `Pending`, кошти не рухаються, і сторони лишаються там, де були: можуть
/// домовитись самі або відкрити новий розгляд за новий депозит.
#[test]
fn puts_the_milestone_back_where_it_was_on_a_status_quo() {
    let fixture = Fixture::build(true, Some(Verdict::StatusQuo), DisputeState::Tallied);
    let result = fixture.ok();

    assert_eq!(fixture.balance(&result, &fixture.vault), total());
    assert_eq!(fixture.balance(&result, &fixture.seller_tokens), 0);
    assert_eq!(fixture.balance(&result, &fixture.buyer_tokens), 0);
    assert_eq!(
        fixture.escrow_state(&result).milestones[DISPUTED as usize].state,
        MilestoneState::Pending
    );
}

#[test]
fn leaves_the_other_milestones_where_they_were() {
    let fixture = Fixture::new();
    let escrow = fixture.escrow_state(&fixture.ok());

    assert_eq!(escrow.milestones[0].state, MilestoneState::Pending);
    assert_eq!(escrow.milestones[2].state, MilestoneState::Pending);
    assert_eq!(escrow.milestones[0].amount, milestones()[0]);
    assert_eq!(escrow.milestones[2].amount, milestones()[2]);
}

// ── FR-013: виконати можна один раз ─────────────────────────────────────────

/// Повторний виклик не переводить кошти вдруге. Тримається це не окремим
/// прапорцем, а тим, що адреса розгляду зникає зі стану віхи разом із
/// виконанням.
#[test]
fn refuses_to_execute_the_same_verdict_twice() {
    let fixture = Fixture::new();
    let first = fixture.ok();

    let mut accounts = fixture.accounts.clone();
    for key in [&fixture.escrow, &fixture.vault, &fixture.seller_tokens] {
        replace(&mut accounts, key, resulting(&first, key).clone());
    }

    let second = mollusk_at(APPEAL_DEADLINE).process_instruction(&fixture.ix(DISPUTED), &accounts);
    assert!(failed_with(
        &second,
        EscrowError::MilestoneNotUnderThisDispute
    ));
}

/// Найтонший спосіб виконати вердикт двічі — принести **інший** спір цієї ж
/// угоди. За `escrow_ref` він зійдеться, за вердиктом теж, і без звірки адреси
/// зі станом віхи давно виконаний розгляд розпорядився б чужою віхою.
#[test]
fn refuses_a_finalized_dispute_that_belongs_to_another_milestone() {
    let fixture = Fixture::new();
    let (other, _) = mesh_dispute_pda(&fixture.integrator, 1);

    let mut hearing = mesh_dispute_state(
        &fixture.integrator,
        1,
        &fixture.escrow,
        &fixture.seller,
        &fixture.buyer,
        milestones()[0],
    );
    hearing.state = DisputeState::Finalized;
    hearing.verdict = Some(Verdict::Claimant);

    let mut accounts = fixture.accounts.clone();
    accounts.push((addr(&other), mesh_dispute_account(&hearing)));

    let result = mollusk_at(APPEAL_DEADLINE)
        .process_instruction(&fixture.ix_over(other, DISPUTED), &accounts);
    assert!(failed_with(
        &result,
        EscrowError::MilestoneNotUnderThisDispute
    ));
}

/// Віха, яку ніхто не оспорював, не має що виконувати.
#[test]
fn refuses_a_milestone_that_is_not_under_dispute() {
    let fixture = Fixture::new();

    let result = mollusk_at(APPEAL_DEADLINE).process_instruction(&fixture.ix(0), &fixture.accounts);
    assert!(failed_with(
        &result,
        EscrowError::MilestoneNotUnderThisDispute
    ));
}

#[test]
fn refuses_a_milestone_that_does_not_exist() {
    let fixture = Fixture::new();

    let result = mollusk_at(APPEAL_DEADLINE).process_instruction(&fixture.ix(9), &fixture.accounts);
    assert!(failed_with(&result, EscrowError::UnknownMilestone));
}

// ── три перевірки над самим спором ──────────────────────────────────────────

/// Чужий спір не розпоряджається цією угодою. Перевірка симетрична тій, що в
/// `open_dispute`: там ескроу доводить, що спір його, підписом; тут — звіркою.
#[test]
fn refuses_a_dispute_opened_over_another_escrow() {
    let fixture = Fixture::new();
    let mut hearing = mesh_dispute_state(
        &fixture.integrator,
        0,
        &Pubkey::new_unique(),
        &fixture.seller,
        &fixture.buyer,
        disputed_amount(),
    );
    hearing.verdict = Some(Verdict::Claimant);

    let mut accounts = fixture.accounts.clone();
    replace(
        &mut accounts,
        &fixture.dispute,
        mesh_dispute_account(&hearing),
    );

    let result = mollusk_at(APPEAL_DEADLINE).process_instruction(&fixture.ix(DISPUTED), &accounts);
    assert!(failed_with(&result, EscrowError::NotOurDispute));
}

#[test]
fn refuses_a_dispute_that_has_no_verdict_yet() {
    let fixture = Fixture::build(true, None, DisputeState::Revealing);

    let result = fixture.settle();
    assert!(failed_with(&result, EscrowError::VerdictPending));
}

/// Виконати вердикт до кінця вікна апеляції означає виконати той, який ще
/// можуть перекрити.
#[test]
fn refuses_a_settlement_before_the_appeal_window_closes() {
    let fixture = Fixture::new();

    let result = mollusk_at(APPEAL_DEADLINE - 1)
        .process_instruction(&fixture.ix(DISPUTED), &fixture.accounts);
    assert!(failed_with(&result, EscrowError::AppealWindowOpen));
}

/// Спір під апеляцією має вердикт першого кола і закритий дедлайн — і саме
/// тому його треба відхиляти окремо: обидві часові перевірки він проходить.
#[test]
fn refuses_a_dispute_that_is_under_appeal() {
    let fixture = Fixture::build(true, Some(Verdict::Claimant), DisputeState::Appealed);

    let result = fixture.settle();
    assert!(failed_with(&result, EscrowError::VerdictUnderAppeal));
}

/// **Не** чекає на `Finalized`. Розрахунок стейків — внутрішня справа
/// VerdictMesh, і зав'язувати на неї виплату означало б тримати чужі кошти
/// замкненими, доки не відпрацює чужий кранк.
#[test]
fn does_not_wait_for_the_arbitration_program_to_settle_its_own_stakes() {
    for state in [DisputeState::Tallied, DisputeState::Finalized] {
        let fixture = Fixture::build(true, Some(Verdict::Claimant), state);
        let result = fixture.ok();

        assert_eq!(
            fixture.balance(&result, &fixture.seller_tokens),
            disputed_amount(),
            "{state:?}"
        );
    }
}

/// Акаунт спору належить VerdictMesh, і Anchor звіряє власника. Підроблений
/// `Dispute`, викладений цією ж програмою, — найпростіший спосіб виписати собі
/// вердикт.
#[test]
fn refuses_a_dispute_account_owned_by_somebody_else() {
    let fixture = Fixture::new();
    let mut hearing = mesh_dispute_state(
        &fixture.integrator,
        0,
        &fixture.escrow,
        &fixture.seller,
        &fixture.buyer,
        disputed_amount(),
    );
    hearing.verdict = Some(Verdict::Claimant);

    let mut forged = mesh_dispute_account(&hearing);
    forged.owner = addr(&ESCROW_PROGRAM);

    let mut accounts = fixture.accounts.clone();
    replace(&mut accounts, &fixture.dispute, forged);

    let result = mollusk_at(APPEAL_DEADLINE).process_instruction(&fixture.ix(DISPUTED), &accounts);
    assert!(result.program_result.is_err());
}

// ── дозвільність і напрямок виплати ─────────────────────────────────────────

/// `SC-005`: «автоматично» означає «без привілейованої людини в контурі».
/// Виконати вердикт може будь-хто, і підпису інструкція не питає взагалі —
/// закривати тут нічого, тож і оренди, за яку варто було б платити кранку,
/// немає.
#[test]
fn takes_no_signature_at_all() {
    let fixture = Fixture::new();

    assert!(
        !fixture
            .ix(DISPUTED)
            .accounts
            .iter()
            .any(|meta| meta.is_signer),
        "executing a verdict must not depend on anyone's key"
    );
}

/// Дозвільний виклик означає, що напрямок виплати не можна підказати ззовні:
/// обидва токен-акаунти прив'язані до своїх власників з угоди, тож підставити
/// свій нічим.
#[test]
fn cannot_be_told_where_to_send_the_money() {
    let fixture = Fixture::new();
    let stranger = Pubkey::new_unique();
    let stranger_tokens = Pubkey::new_unique();

    let mut accounts = fixture.accounts.clone();
    accounts.push((
        addr(&stranger_tokens),
        token_account(&fixture.mint, &stranger, 0),
    ));

    let result = mollusk_at(APPEAL_DEADLINE).process_instruction(
        &fixture.ix_paying(
            fixture.dispute,
            DISPUTED,
            fixture.buyer_tokens,
            stranger_tokens,
        ),
        &accounts,
    );
    assert!(result.program_result.is_err());
}

#[test]
fn emits_the_settlement_event() {
    let fixture = Fixture::new();
    let (mut mollusk, logs) = mollusk_with_logs();
    mollusk.sysvars.clock.unix_timestamp = APPEAL_DEADLINE;

    let result = mollusk.process_instruction(&fixture.ix(DISPUTED), &fixture.accounts);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let events: Vec<MilestoneSettled> = emitted(&logs);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].escrow, fixture.escrow);
    assert_eq!(events[0].milestone, DISPUTED);
    assert_eq!(events[0].dispute, fixture.dispute);
    assert_eq!(events[0].winner, Some(fixture.seller));
    assert_eq!(events[0].amount, disputed_amount());
}

/// Статус-кво теж подія: спостерігач мусить бачити, що розгляд закінчився, а
/// кошти не рухались. Порожній `winner` і є той факт.
#[test]
fn announces_a_status_quo_as_a_settlement_that_moved_nothing() {
    let fixture = Fixture::build(true, Some(Verdict::StatusQuo), DisputeState::Tallied);
    let (mut mollusk, logs) = mollusk_with_logs();
    mollusk.sysvars.clock.unix_timestamp = APPEAL_DEADLINE;

    let result = mollusk.process_instruction(&fixture.ix(DISPUTED), &fixture.accounts);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let events: Vec<MilestoneSettled> = emitted(&logs);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].winner, None);
    assert_eq!(events[0].amount, 0);
}
