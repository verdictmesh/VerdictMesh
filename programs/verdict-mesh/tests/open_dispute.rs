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
    events::{DepositCollected, DisputeOpened},
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
    claimant_tokens: Pubkey,
    respondent: Pubkey,
    mint: Pubkey,
    policy: Policy,
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
        let claimant_tokens = Pubkey::new_unique();
        let respondent = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        let (integrator, integrator_bump) = integrator_pda(&authority);
        let dispute = dispute_pda(&integrator, 0).0;

        // Перші п'ять — у порядку, на який спираються тести, що підміняють
        // акаунт за індексом. Решта дописана в хвіст: mollusk шукає акаунти за
        // ключем, а не за позицією.
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
            (addr(&dispute), missing()),
            keyed_account_for_system_program(),
            (
                addr(&config_pda().0),
                config_account(&mint, &Pubkey::new_unique(), &Pubkey::new_unique()),
            ),
            (addr(&mint), settlement_mint()),
            (addr(&claimant), wallet(1_000_000_000)),
            (
                addr(&claimant_tokens),
                // Удвічі більше за депозит: тест «не вистачило» має ламати
                // баланс сам, а не тримати фікстуру на межі.
                token_account(&mint, &claimant, 2 * policy.deposit),
            ),
            (addr(&dispute_vault_pda(&dispute).0), missing()),
            keyed_account_for_token_program(),
        ];

        Self {
            payer,
            escrow_program,
            escrow,
            integrator,
            claimant,
            claimant_tokens,
            respondent,
            mint,
            policy,
            accounts,
        }
    }

    /// Набір акаунтів під інший спір. Підмінити треба **обидва** акаунти, що
    /// виводяться з його адреси — сам спір і його сховище: інакше інструкція
    /// падала б на сховищі, а виглядало б це як відмова, яку тест нібито
    /// доводить.
    fn accounts_for(&self, dispute: &Pubkey) -> Vec<(Address, Account)> {
        let mut accounts = self.accounts.clone();
        accounts[3] = (addr(dispute), missing());
        accounts.push((addr(&dispute_vault_pda(dispute).0), missing()));
        accounts
    }

    fn dispute(&self, dispute_id: u64) -> Pubkey {
        dispute_pda(&self.integrator, dispute_id).0
    }

    fn ix(&self, dispute_id: u64) -> Instruction {
        self.ix_paid_by(dispute_id, self.claimant, self.claimant_tokens)
    }

    fn ix_paid_by(&self, dispute_id: u64, depositor: Pubkey, tokens: Pubkey) -> Instruction {
        let dispute = self.dispute(dispute_id);

        anchor_ix(
            verdict_mesh::accounts::OpenDispute {
                payer: self.payer,
                config: config_pda().0,
                settlement_mint: self.mint,
                depositor,
                integrator: self.integrator,
                escrow: self.escrow,
                dispute,
                depositor_tokens: tokens,
                dispute_vault: dispute_vault_pda(&dispute).0,
                token_program: TOKEN_PROGRAM,
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

    fn vault(&self, dispute_id: u64) -> Pubkey {
        dispute_vault_pda(&self.dispute(dispute_id)).0
    }

    /// Той самий набір акаунтів, інші аргументи. Тести меж предмета спору
    /// ламають саме аргументи, і виписувати заради цього одинадцять акаунтів
    /// щоразу означало б, що наступний акаунт інструкції доведеться дописати в
    /// чотирьох місцях — а забути в одному.
    fn ix_claiming(
        &self,
        claimant: Pubkey,
        respondent: Pubkey,
        amount: u64,
        claimant_claim_hash: [u8; 32],
        respondent_claim_hash: [u8; 32],
    ) -> Instruction {
        let dispute = self.dispute(0);

        anchor_ix(
            verdict_mesh::accounts::OpenDispute {
                payer: self.payer,
                config: config_pda().0,
                settlement_mint: self.mint,
                depositor: self.claimant,
                integrator: self.integrator,
                escrow: self.escrow,
                dispute,
                depositor_tokens: self.claimant_tokens,
                dispute_vault: dispute_vault_pda(&dispute).0,
                token_program: TOKEN_PROGRAM,
                system_program: SYSTEM_PROGRAM,
            },
            verdict_mesh::instruction::OpenDispute {
                claimant,
                respondent,
                amount,
                claimant_claim_hash,
                respondent_claim_hash,
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

    let mut accounts = fixture.accounts_for(&fixture.dispute(1));
    accounts[1] = (
        addr(&fixture.integrator),
        resulting(&first, &fixture.integrator).clone(),
    );
    // Токен-акаунт позивача — уже після першого депозиту: другий спір
    // оплачується з того, що лишилось.
    replace(
        &mut accounts,
        &fixture.claimant_tokens,
        resulting(&first, &fixture.claimant_tokens).clone(),
    );

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

    let accounts = fixture.accounts_for(&fixture.dispute(1));

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
            config: config_pda().0,
            settlement_mint: fixture.mint,
            depositor: fixture.claimant,
            integrator: fixture.integrator,
            escrow: fixture.escrow,
            dispute: other.dispute(0),
            depositor_tokens: fixture.claimant_tokens,
            dispute_vault: dispute_vault_pda(&other.dispute(0)).0,
            token_program: TOKEN_PROGRAM,
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

    let accounts = fixture.accounts_for(&other.dispute(0));

    let result = mollusk().process_instruction(&ix, &accounts);
    assert!(result.program_result.is_err());
}

// ── межі предмета спору ─────────────────────────────────────────────────────

/// Той самий ключ по обидва боки робить безглуздими і вердикт, і розподіл:
/// сторона програє сама собі.
#[test]
fn rejects_identical_parties() {
    let fixture = Fixture::new();
    let ix = fixture.ix_claiming(
        fixture.claimant,
        fixture.claimant,
        AMOUNT,
        [1u8; 32],
        [2u8; 32],
    );

    let result = mollusk().process_instruction(&ix, &fixture.accounts);
    assert!(failed_with(&result, VerdictMeshError::InvalidParties));
}

/// `FR-004`: спір відкривається над конкретним замкненим залишком. Нульовий
/// залишок — це не спір, а розгляд, за який хтось платить депозит ні за що.
#[test]
fn rejects_a_dispute_over_nothing() {
    let fixture = Fixture::new();
    let ix = fixture.ix_claiming(
        fixture.claimant,
        fixture.respondent,
        0,
        [1u8; 32],
        [2u8; 32],
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
        let ix = fixture.ix_claiming(
            fixture.claimant,
            fixture.respondent,
            AMOUNT,
            claimant_claim_hash,
            respondent_claim_hash,
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

/// Сума депозиту фіксується знімком політики (`FR-003`), а не читається з
/// `Integrator` під час розрахунку: інтегратор, який підняв ціну розгляду
/// назавтра, не має піднімати її заднім числом тому, хто вже платить.
#[test]
fn carries_the_deposit_amount_in_the_snapshot() {
    let fixture = Fixture::new();
    let result = fixture.open();

    let dispute: Dispute = decode(resulting(&result, &fixture.dispute(0)));
    assert_eq!(dispute.policy.deposit, demo_policy().deposit);
}

// ── FR-026: депозит за розгляд ──────────────────────────────────────────────

/// Головне про депозит: спору без оплаченого розгляду не існує. Сховище
/// створюється й наповнюється тією ж транзакцією, що відкриває спір, тож
/// панель ніколи не працює в борг.
#[test]
fn collects_the_deposit_into_the_vault_of_this_dispute() {
    let fixture = Fixture::new();
    let result = fixture.open();
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let vault = token_state(resulting(&result, &fixture.vault(0)));
    assert_eq!(vault.amount, fixture.policy.deposit);

    let payer = token_state(resulting(&result, &fixture.claimant_tokens));
    assert_eq!(payer.amount, fixture.policy.deposit);
}

/// Владу над сховищем має PDA програми, а не той, хто платив. Інакше позивач
/// забрав би депозит назад рівно тоді, коли розгляд повернувся проти нього, —
/// і присяжні лишились би без оплати саме в спорах, які варто було виграти.
#[test]
fn puts_the_deposit_under_the_authority_of_the_program() {
    let fixture = Fixture::new();
    let result = fixture.open();

    let vault = token_state(resulting(&result, &fixture.vault(0)));
    assert_eq!(vault.owner, addr(&config_pda().0));
    assert_eq!(vault.mint, addr(&fixture.mint));
}

/// Сховище на кожен спір своє. Спільне не дало б відповіді на питання, чиї саме
/// кошти в ньому лежать, — а воно постане з апеляційною заставою (`FR-021`),
/// яку повертають поіменно.
#[test]
fn gives_every_dispute_a_vault_of_its_own() {
    let fixture = Fixture::new();

    assert_ne!(fixture.vault(0), fixture.vault(1));
}

/// `FR-026`: платить **сторона, яка відкриває спір**. Без цієї рівності ескроу
/// відкривав би спір «від імені» позивача, а списував з чужого гаманця — і
/// `FR-026a` не мав би на чому триматись: вартість розгляду несла б людина,
/// яка про спір не знала.
#[test]
fn rejects_a_depositor_who_is_not_the_claimant() {
    let fixture = Fixture::new();
    let stranger = Pubkey::new_unique();
    let stranger_tokens = Pubkey::new_unique();

    let mut accounts = fixture.accounts.clone();
    accounts.push((addr(&stranger), wallet(1_000_000_000)));
    accounts.push((
        addr(&stranger_tokens),
        token_account(&fixture.mint, &stranger, usdc(1_000)),
    ));

    let result =
        mollusk().process_instruction(&fixture.ix_paid_by(0, stranger, stranger_tokens), &accounts);

    assert!(failed_with(&result, VerdictMeshError::NotTheDepositor));
}

/// Позивач без депозиту спору не відкриває. Перевірки в програмі для цього
/// немає й не треба — переказ падає сам, — але тест мусить довести, що падає
/// **вся** транзакція: спір, записаний повз несплачений депозит, був би
/// розглядом за чужий рахунок.
#[test]
fn refuses_to_open_a_dispute_the_claimant_cannot_pay_for() {
    let fixture = Fixture::new();

    let mut accounts = fixture.accounts.clone();
    replace(
        &mut accounts,
        &fixture.claimant_tokens,
        token_account(&fixture.mint, &fixture.claimant, fixture.policy.deposit - 1),
    );

    let result = mollusk().process_instruction(&fixture.ix(0), &accounts);
    assert!(result.program_result.is_err());
}

/// Депозит списується в розрахунковому активі протоколу (`FR-011a`), а не в
/// активі спору. Чужий мінт відхиляється прив'язкою до `Config`: інакше
/// сторона заплатила б власним токеном, який нічого не вартий, і присяжні
/// отримали б за розгляд саме його.
#[test]
fn rejects_a_deposit_in_another_asset() {
    let fixture = Fixture::new();
    let other_mint = Pubkey::new_unique();
    let other_tokens = Pubkey::new_unique();

    let mut accounts = fixture.accounts.clone();
    accounts.push((addr(&other_mint), settlement_mint()));
    accounts.push((
        addr(&other_tokens),
        token_account(&other_mint, &fixture.claimant, usdc(1_000)),
    ));

    let mut ix = fixture.ix(0);
    for meta in &mut ix.accounts {
        if meta.pubkey == addr(&fixture.mint) {
            meta.pubkey = addr(&other_mint);
        }
        if meta.pubkey == addr(&fixture.claimant_tokens) {
            meta.pubkey = addr(&other_tokens);
        }
    }

    let result = mollusk().process_instruction(&ix, &accounts);
    assert!(result.program_result.is_err());
}

/// `FR-029`: скільки з сторони взяли, видно з подій, а не лише з політики, яку
/// інтегратор змінить назавтра.
#[test]
fn emits_the_deposit_event() {
    let fixture = Fixture::new();
    let (mollusk, logs) = mollusk_with_logs();

    let result = mollusk.process_instruction(&fixture.ix(0), &fixture.accounts);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let events: Vec<DepositCollected> = emitted(&logs);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].dispute, fixture.dispute(0));
    assert_eq!(events[0].depositor, fixture.claimant);
    assert_eq!(events[0].amount, fixture.policy.deposit);
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
