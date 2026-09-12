//! T012 — `open_dispute` (`FR-003`, `FR-004`, `FR-005`, `FR-029`).
//!
//! Найважливіше в цій інструкції — не те, що вона записує, а хто має право її
//! викликати. `Dispute.escrow_ref` — єдине, за чим ескроу впізнає «свій» спір
//! перед тим, як розподілити кошти (`FR-012`). Якби спір міг відкрити хто
//! завгодно, вистачило б підставити чужий `escrow_ref`, щоб змусити чужий
//! ескроу виконати вигаданий вердикт. Тому спір відкриває **підпис самої
//! програми ескроу**, і саме це перевіряють тести межі нижче.

#[allow(dead_code)]
#[path = "harness.rs"]
mod harness;

use anchor_lang::solana_program::pubkey::Pubkey;
use harness::*;
use mollusk_svm::{program::keyed_account_for_system_program, result::InstructionResult};
use solana_account::Account;
use solana_address::Address;
use solana_instruction::Instruction;
use verdict_mesh::{
    events::DisputeOpened,
    state::{Dispute, DisputeState, Integrator, Policy},
    VerdictMeshError,
};

const AMOUNT: u64 = 250;

struct Fixture {
    payer: Pubkey,
    escrow_program: Pubkey,
    escrow: Pubkey,
    integrator: Pubkey,
    claimant: Pubkey,
    respondent: Pubkey,
    accounts: Vec<(Address, Account)>,
}

impl Fixture {
    fn new() -> Self {
        Self::with_policy(demo_policy())
    }

    fn with_policy(policy: Policy) -> Self {
        let payer = Pubkey::new_unique();
        let authority = Pubkey::new_unique();
        let escrow_program = Pubkey::new_unique();
        let escrow = Pubkey::new_unique();
        let claimant = Pubkey::new_unique();
        let respondent = Pubkey::new_unique();
        let (integrator, integrator_bump) = integrator_pda(&authority);

        let accounts = vec![
            (addr(&payer), wallet(10_000_000_000)),
            (
                addr(&integrator),
                program_account(&Integrator {
                    authority,
                    escrow_program,
                    policy,
                    dispute_count: 0,
                    bump: integrator_bump,
                }),
            ),
            (addr(&escrow), escrow_account(&escrow_program)),
            (addr(&dispute_pda(&integrator, 0).0), missing()),
            keyed_account_for_system_program(),
        ];

        Self {
            payer,
            escrow_program,
            escrow,
            integrator,
            claimant,
            respondent,
            accounts,
        }
    }

    fn dispute(&self, dispute_id: u64) -> Pubkey {
        dispute_pda(&self.integrator, dispute_id).0
    }

    fn ix(&self, dispute_id: u64) -> Instruction {
        anchor_ix(
            verdict_mesh::accounts::OpenDispute {
                payer: self.payer,
                integrator: self.integrator,
                escrow: self.escrow,
                dispute: self.dispute(dispute_id),
                system_program: SYSTEM_PROGRAM,
            },
            verdict_mesh::instruction::OpenDispute {
                claimant: self.claimant,
                respondent: self.respondent,
                amount: AMOUNT,
                claimant_claim_hash: [1u8; 32],
                respondent_claim_hash: [2u8; 32],
            },
        )
    }

    fn open(&self) -> InstructionResult {
        mollusk().process_instruction(&self.ix(0), &self.accounts)
    }
}

/// Акаунт ескроу — звичайний PDA чужої програми: власник і є доказом того, що
/// виклик прийшов від неї.
fn escrow_account(escrow_program: &Pubkey) -> Account {
    Account {
        lamports: 10_000_000,
        data: vec![0u8; 64],
        owner: addr(escrow_program),
        executable: false,
        rent_epoch: 0,
    }
}

// ── що записується ──────────────────────────────────────────────────────────

#[test]
fn records_the_parties_the_amount_and_the_escrow_that_opened_it() {
    let fixture = Fixture::new();
    let result = fixture.open();
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let dispute: Dispute = decode(resulting(&result, &fixture.dispute(0)));
    assert_eq!(dispute.integrator, fixture.integrator);
    assert_eq!(dispute.dispute_id, 0);
    assert_eq!(dispute.escrow_ref, fixture.escrow);
    assert_eq!(dispute.claimant, fixture.claimant);
    assert_eq!(dispute.respondent, fixture.respondent);
    assert_eq!(dispute.amount, AMOUNT);
    assert_eq!(dispute.claimant_claim_hash, [1u8; 32]);
    assert_eq!(dispute.respondent_claim_hash, [2u8; 32]);
    assert_eq!(dispute.bump, dispute_pda(&fixture.integrator, 0).1);
}

/// `FR-003`: політика фіксується в момент відкриття. Копія лежить у самому
/// спорі, а не посиланням на інтегратора — інакше «незмінна до фіналізації»
/// трималася б лише на тому, що інтегратора нікому змінити.
#[test]
fn snapshots_the_policy_into_the_dispute() {
    let fixture = Fixture::new();
    let result = fixture.open();

    let dispute: Dispute = decode(resulting(&result, &fixture.dispute(0)));
    assert_eq!(dispute.policy, demo_policy());
}

/// Панель відбирається окремою межею (`FR-006a`, T016). До того спір існує з
/// порожньою панеллю — і місце під неї вже виділене, бо збільшити акаунт після
/// створення нічим.
#[test]
fn opens_in_the_committing_state_with_room_for_the_extended_panel() {
    let fixture = Fixture::new();
    let result = fixture.open();

    let account = resulting(&result, &fixture.dispute(0));
    let dispute: Dispute = decode(account);

    assert_eq!(dispute.state, DisputeState::Committing);
    assert!(dispute.panel.is_empty());
    assert!(dispute.verdict.is_none());
    assert!(!dispute.settled);
    assert!(!dispute.escalated);
    assert_eq!(
        account.data.len(),
        Dispute::space(demo_policy().extended_panel_size)
    );
}

/// Дедлайни рахуються від часу відкриття, а не «від нуля»: вікно розкриття
/// починається там, де закінчилось вікно подання.
#[test]
fn derives_the_deadlines_from_the_snapshotted_windows() {
    let fixture = Fixture::new();
    let result = fixture.open();

    let policy = demo_policy();
    let dispute: Dispute = decode(resulting(&result, &fixture.dispute(0)));
    assert_eq!(dispute.opened_at, NOW);
    assert_eq!(dispute.commit_deadline, NOW + policy.commit_window);
    assert_eq!(
        dispute.reveal_deadline,
        NOW + policy.commit_window + policy.reveal_window
    );
}

/// Ентропія відбору панелі фіксується тут, а не там, де відбувається сам
/// відбір (`FR-006`, T016): інакше відбір можна було б переграти, повторюючи
/// спробу зі слота в слот, доки панель не сподобається.
///
/// Береться слот **перед** поточним: хеш поточного ще не існує, тож відбір у
/// тій самій транзакції його не знайшов би.
#[test]
fn pins_the_entropy_slot_of_the_panel_at_opening() {
    let fixture = Fixture::new();
    let result = fixture.open();

    let dispute: Dispute = decode(resulting(&result, &fixture.dispute(0)));
    assert_eq!(dispute.entropy_slot, SLOT - 1);
}

/// Вікно апеляції відкривається від вердикту, а не від відкриття спору, тож на
/// цьому кроці його дедлайн ще не існує.
#[test]
fn leaves_the_appeal_deadline_unset() {
    let fixture = Fixture::new();
    let result = fixture.open();

    let dispute: Dispute = decode(resulting(&result, &fixture.dispute(0)));
    assert_eq!(dispute.appeal_deadline, 0);
}

/// Нумерація йде з лічильника інтегратора: спір №1 неможливо створити, поки
/// не існує №0, і два спори не можуть отримати одну адресу.
#[test]
fn numbers_disputes_from_the_integrator_counter() {
    let fixture = Fixture::new();
    let first = fixture.open();
    assert!(first.program_result.is_ok(), "{:?}", first.raw_result);

    let integrator: Integrator = decode(resulting(&first, &fixture.integrator));
    assert_eq!(integrator.dispute_count, 1);

    let mut accounts = fixture.accounts.clone();
    accounts[1] = (
        addr(&fixture.integrator),
        resulting(&first, &fixture.integrator).clone(),
    );
    accounts[3] = (addr(&fixture.dispute(1)), missing());

    let second = mollusk().process_instruction(&fixture.ix(1), &accounts);
    assert!(second.program_result.is_ok(), "{:?}", second.raw_result);

    let dispute: Dispute = decode(resulting(&second, &fixture.dispute(1)));
    assert_eq!(dispute.dispute_id, 1);
}

/// Спір із номером, який лічильник ще не видав, відкрити не можна — інакше
/// нумерація перестала б бути щільною, а `dispute_count` — правдою про те,
/// скільки спорів існує.
///
/// Перевіркою тут є сам seed: адреса спору — функція від лічильника, тож для
/// номера попереду лічильника просто немає адреси, за якою його створити.
/// Помилку кидає констрейнт Anchor, і її код навмисно не фіксуємо — він не
/// частина контракту програми, на відміну від того, що операція не проходить.
#[test]
fn rejects_a_dispute_id_ahead_of_the_counter() {
    let fixture = Fixture::new();

    let mut accounts = fixture.accounts.clone();
    accounts[3] = (addr(&fixture.dispute(1)), missing());

    let result = mollusk().process_instruction(&fixture.ix(1), &accounts);
    assert!(result.program_result.is_err());
}

// ── FR-029: подія ───────────────────────────────────────────────────────────

/// Подія — єдине, за чим watcher (T027) дізнається про спір. Тому перевіряємо
/// не «подія є», а що в ній лежить рівно те, чим спір відкрили.
#[test]
fn emits_the_opening_event() {
    let fixture = Fixture::new();
    let (mollusk, logs) = mollusk_with_logs();

    let result = mollusk.process_instruction(&fixture.ix(0), &fixture.accounts);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let events: Vec<DisputeOpened> = emitted(&logs);
    assert_eq!(events.len(), 1);

    let event = &events[0];
    assert_eq!(event.dispute, fixture.dispute(0));
    assert_eq!(event.integrator, fixture.integrator);
    assert_eq!(event.escrow_ref, fixture.escrow);
    assert_eq!(event.claimant, fixture.claimant);
    assert_eq!(event.respondent, fixture.respondent);
    assert_eq!(event.amount, AMOUNT);
    assert_eq!(event.opened_at, NOW);
    assert!(!event.optimistic);
}

// ── межа повноважень ────────────────────────────────────────────────────────

/// Головний тест цього файлу. Ескроу довіряє полю `escrow_ref` і за ним
/// розподіляє кошти. Якби спір відкривав будь-хто, підставлений `escrow_ref`
/// змусив би чужий ескроу виконати вигаданий вердикт.
#[test]
fn rejects_an_escrow_that_belongs_to_another_program() {
    let fixture = Fixture::new();
    let impostor = Pubkey::new_unique();

    let mut accounts = fixture.accounts.clone();
    accounts[2] = (addr(&fixture.escrow), escrow_account(&impostor));

    let result = mollusk().process_instruction(&fixture.ix(0), &accounts);
    assert!(result.program_result.is_err());
}

/// Власності замало: акаунт чужої програми може прочитати будь-хто. Спір
/// відкриває саме та транзакція, у якій програма ескроу підписала своїм PDA.
#[test]
fn rejects_an_escrow_that_did_not_sign() {
    let fixture = Fixture::new();

    let mut ix = fixture.ix(0);
    let escrow = addr(&fixture.escrow);
    for meta in ix.accounts.iter_mut() {
        if meta.pubkey == escrow {
            meta.is_signer = false;
        }
    }

    let result = mollusk().process_instruction(&ix, &fixture.accounts);
    assert!(result.program_result.is_err());
}

#[test]
fn requires_the_escrow_to_sign_by_default() {
    let fixture = Fixture::new();
    let escrow = addr(&fixture.escrow);

    let meta = fixture
        .ix(0)
        .accounts
        .into_iter()
        .find(|meta| meta.pubkey == escrow)
        .expect("the escrow must be among the instruction accounts");
    assert!(meta.is_signer);
}

/// Спір належить конкретному інтегратору: PDA спору виводиться з нього, тож
/// підставлений чужий акаунт інтегратора не сходиться з адресою спору.
#[test]
fn rejects_a_dispute_that_does_not_belong_to_the_given_integrator() {
    let fixture = Fixture::new();
    let other = Fixture::new();

    let ix = anchor_ix(
        verdict_mesh::accounts::OpenDispute {
            payer: fixture.payer,
            integrator: fixture.integrator,
            escrow: fixture.escrow,
            dispute: other.dispute(0),
            system_program: SYSTEM_PROGRAM,
        },
        verdict_mesh::instruction::OpenDispute {
            claimant: fixture.claimant,
            respondent: fixture.respondent,
            amount: AMOUNT,
            claimant_claim_hash: [1u8; 32],
            respondent_claim_hash: [2u8; 32],
        },
    );

    let mut accounts = fixture.accounts.clone();
    accounts[3] = (addr(&other.dispute(0)), missing());

    let result = mollusk().process_instruction(&ix, &accounts);
    assert!(result.program_result.is_err());
}

// ── межі предмета спору ─────────────────────────────────────────────────────

/// Той самий ключ по обидва боки робить безглуздими і вердикт, і розподіл:
/// сторона програє сама собі.
#[test]
fn rejects_identical_parties() {
    let fixture = Fixture::new();
    let ix = anchor_ix(
        verdict_mesh::accounts::OpenDispute {
            payer: fixture.payer,
            integrator: fixture.integrator,
            escrow: fixture.escrow,
            dispute: fixture.dispute(0),
            system_program: SYSTEM_PROGRAM,
        },
        verdict_mesh::instruction::OpenDispute {
            claimant: fixture.claimant,
            respondent: fixture.claimant,
            amount: AMOUNT,
            claimant_claim_hash: [1u8; 32],
            respondent_claim_hash: [2u8; 32],
        },
    );

    let result = mollusk().process_instruction(&ix, &fixture.accounts);
    assert!(failed_with(&result, VerdictMeshError::InvalidParties));
}

/// `FR-004`: спір відкривається над конкретним замкненим залишком. Нульовий
/// залишок — це не спір, а розгляд, за який хтось платить депозит ні за що.
#[test]
fn rejects_a_dispute_over_nothing() {
    let fixture = Fixture::new();
    let ix = anchor_ix(
        verdict_mesh::accounts::OpenDispute {
            payer: fixture.payer,
            integrator: fixture.integrator,
            escrow: fixture.escrow,
            dispute: fixture.dispute(0),
            system_program: SYSTEM_PROGRAM,
        },
        verdict_mesh::instruction::OpenDispute {
            claimant: fixture.claimant,
            respondent: fixture.respondent,
            amount: 0,
            claimant_claim_hash: [1u8; 32],
            respondent_claim_hash: [2u8; 32],
        },
    );

    let result = mollusk().process_instruction(&ix, &fixture.accounts);
    assert!(failed_with(&result, VerdictMeshError::InvalidAmount));
}

/// `FR-005`: кожна сторона подає твердження. Нульовий відбиток — це не «порожнє
/// твердження», а відсутнє: присяжний побачив би позицію лише однієї сторони й
/// не мав би як це помітити.
#[test]
fn rejects_a_missing_statement_from_either_side() {
    let fixture = Fixture::new();

    for (claimant_claim_hash, respondent_claim_hash) in
        [([0u8; 32], [2u8; 32]), ([1u8; 32], [0u8; 32])]
    {
        let ix = anchor_ix(
            verdict_mesh::accounts::OpenDispute {
                payer: fixture.payer,
                integrator: fixture.integrator,
                escrow: fixture.escrow,
                dispute: fixture.dispute(0),
                system_program: SYSTEM_PROGRAM,
            },
            verdict_mesh::instruction::OpenDispute {
                claimant: fixture.claimant,
                respondent: fixture.respondent,
                amount: AMOUNT,
                claimant_claim_hash,
                respondent_claim_hash,
            },
        );

        let result = mollusk().process_instruction(&ix, &fixture.accounts);
        assert!(failed_with(&result, VerdictMeshError::MissingClaim));
    }
}

/// Розмір акаунта йде за політикою конкретного інтегратора, а не за стелею:
/// більша розширена панель — більший спір.
#[test]
fn sizes_the_dispute_by_the_policy_of_its_integrator() {
    let mut policy = demo_policy();
    policy.panel_size = 7;
    policy.quorum = 4;
    policy.extended_panel_size = 11;
    policy.extended_quorum = 6;

    let fixture = Fixture::with_policy(policy);
    let result = fixture.open();
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let account = resulting(&result, &fixture.dispute(0));
    assert_eq!(account.data.len(), Dispute::space(11));
    assert!(account.data.len() > Dispute::space(demo_policy().extended_panel_size));
}

/// Депозит за розгляд (`FR-004`, `FR-026`) на цьому кроці ще не переказується:
/// сховище депозитів — T021. Сума, яку сторона винна, уже зафіксована знімком
/// політики, і саме за нею T021 її й спише.
#[test]
fn carries_the_deposit_amount_in_the_snapshot() {
    let fixture = Fixture::new();
    let result = fixture.open();

    let dispute: Dispute = decode(resulting(&result, &fixture.dispute(0)));
    assert_eq!(dispute.policy.deposit, demo_policy().deposit);
}

#[test]
fn keeps_the_escrow_program_out_of_the_instruction() {
    let fixture = Fixture::new();
    let program = addr(&fixture.escrow_program);

    // Програму ескроу не передають: її ідентифікатор уже лежить в акаунті
    // інтегратора, і другий шлях повідомити його був би другим шляхом збрехати.
    assert!(fixture
        .ix(0)
        .accounts
        .iter()
        .all(|meta| meta.pubkey != program));
}
