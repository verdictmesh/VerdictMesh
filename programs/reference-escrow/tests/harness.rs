//! Обв'язка для тестів ескроу.
//!
//! Підключається з інших тестових бінарників так:
//! `#[allow(dead_code)] #[path = "harness.rs"] mod harness;`
//!
//! **Обидві програми справжні.** Відкриття спору — це CPI у VerdictMesh, і
//! підмінити його заглушкою означало б перевіряти власну фікстуру: чи прийме
//! VerdictMesh підпис PDA ескроу, чи зійдеться політика, чи вистачить депозиту
//! — усе це вирішує **той** байткод, а не цей.

use anchor_lang::{
    solana_program::pubkey::Pubkey, AccountDeserialize, AnchorDeserialize, AnchorSerialize,
    Discriminator, InstructionData, Space, ToAccountMetas,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use mollusk_svm::{program::loader_keys::LOADER_V3, result::InstructionResult, Mollusk};
use mollusk_svm_programs_token::token;
use reference_escrow::EscrowError;
use solana_account::Account;
use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};
use solana_instruction_error::InstructionError;
use solana_program_pack::Pack;
use solana_svm_log_collector::LogCollector;
use spl_token_interface::state::{
    Account as SplTokenAccount, AccountState as SplAccountState, Mint as SplMint,
};
use std::{cell::RefCell, rc::Rc};
use verdict_mesh::state::{Config, Dispute, DisputeState, Integrator, Policy};

pub const ESCROW_PROGRAM: Pubkey = reference_escrow::ID;
pub const MESH_PROGRAM: Pubkey = verdict_mesh::ID;

/// mollusk 0.15 говорить типами нового покоління крейтів Solana (`Address`), а
/// Anchor 0.32.1 — старого (`Pubkey`). Той самий 32-байтовий ключ у двох
/// незалежних новотипах; переклад тримаємо в одному місці.
pub fn addr(key: &Pubkey) -> Address {
    Address::from(key.to_bytes())
}

pub const DECIMALS: u32 = 6;

pub fn usdc(amount: u64) -> u64 {
    amount * 10u64.pow(DECIMALS)
}

/// Фіксований «зараз» — та сама мить, що в тестах VerdictMesh.
pub const NOW: i64 = 1_800_000_000;
pub const SLOT: u64 = 372_000_000;

fn elf(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/deploy")
        .join(name);

    std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "cannot read {}: {error}. Run `anchor build` (WSL) before the program tests.",
            path.display()
        )
    })
}

pub fn mollusk() -> Mollusk {
    let mut mollusk = Mollusk::default();
    mollusk.add_program_with_loader_and_elf(
        &addr(&ESCROW_PROGRAM),
        &LOADER_V3,
        &elf("reference_escrow.so"),
    );
    mollusk.add_program_with_loader_and_elf(
        &addr(&MESH_PROGRAM),
        &LOADER_V3,
        &elf("verdict_mesh.so"),
    );
    token::add_program(&mut mollusk);
    mollusk.sysvars.clock.unix_timestamp = NOW;
    mollusk.sysvars.clock.slot = SLOT;
    mollusk
}

pub fn mollusk_with_logs() -> (Mollusk, Rc<RefCell<LogCollector>>) {
    let mut mollusk = mollusk();
    let logs = LogCollector::new_ref();
    mollusk.logger = Some(logs.clone());
    (mollusk, logs)
}

/// Події одного типу з логів, у порядку появи.
pub fn emitted<E: AnchorDeserialize + Discriminator>(logs: &Rc<RefCell<LogCollector>>) -> Vec<E> {
    logs.borrow()
        .get_recorded_content()
        .iter()
        .filter_map(|line| line.strip_prefix("Program data: "))
        .filter_map(|payload| BASE64.decode(payload).ok())
        .filter_map(|bytes| {
            let mut body = bytes.strip_prefix(E::DISCRIMINATOR)?;
            E::deserialize(&mut body).ok()
        })
        .collect()
}

pub fn anchor_ix<A: ToAccountMetas, D: InstructionData>(
    program: &Pubkey,
    accounts: A,
    args: D,
) -> Instruction {
    Instruction {
        program_id: addr(program),
        accounts: accounts
            .to_account_metas(None)
            .iter()
            .map(|meta| AccountMeta {
                pubkey: addr(&meta.pubkey),
                is_signer: meta.is_signer,
                is_writable: meta.is_writable,
            })
            .collect(),
        data: args.data(),
    }
}

pub fn resulting<'a>(result: &'a InstructionResult, key: &Pubkey) -> &'a Account {
    let key = addr(key);
    result
        .resulting_accounts
        .iter()
        .find(|(candidate, _)| *candidate == key)
        .map(|(_, account)| account)
        .unwrap_or_else(|| panic!("{key} is not among the resulting accounts"))
}

pub fn decode<T: AccountDeserialize>(account: &Account) -> T {
    T::try_deserialize(&mut account.data.as_slice())
        .unwrap_or_else(|error| panic!("cannot deserialize the account: {error}"))
}

/// Провал саме з очікуваною помилкою ескроу, а не «якийсь провал». Тест, що
/// приймає будь-яку помилку, зеленіє й тоді, коли інструкція падає на
/// відсутньому акаунті замість перевірки, яку він нібито доводить.
pub fn failed_with(result: &InstructionResult, expected: EscrowError) -> bool {
    matches!(&result.raw_result, Err(InstructionError::Custom(code)) if *code == u32::from(expected))
}

/// Те саме для помилок VerdictMesh: спір відкриває чужа програма, і частина
/// відмов належить їй.
pub fn failed_with_mesh(
    result: &InstructionResult,
    expected: verdict_mesh::VerdictMeshError,
) -> bool {
    matches!(&result.raw_result, Err(InstructionError::Custom(code)) if *code == u32::from(expected))
}

pub const SYSTEM_PROGRAM: Pubkey = anchor_lang::solana_program::system_program::ID;
pub const TOKEN_PROGRAM: Pubkey = anchor_spl::token::ID;

pub fn wallet(lamports: u64) -> Account {
    Account::new(lamports, 0, &addr(&SYSTEM_PROGRAM))
}

/// Акаунт, якого ще немає. Mollusk вимагає запису під кожен ключ інструкції,
/// тож «немає акаунта» доводиться передавати явно нулями.
pub fn missing() -> Account {
    Account::default()
}

/// Акаунт, який колись створила названа програма. Місце виділяється під
/// **повний** розмір типу: `Option::None` займає байт, `Some` — два, і акаунт,
/// обрізаний по `None`, не приймає запису `Some`.
pub fn program_account<T: AnchorSerialize + Discriminator + Space>(
    owner: &Pubkey,
    state: &T,
) -> Account {
    let mut data = T::DISCRIMINATOR.to_vec();
    state
        .serialize(&mut data)
        .expect("account state must serialize");
    data.resize(data.len().max(T::DISCRIMINATOR.len() + T::INIT_SPACE), 0);

    Account {
        lamports: 10_000_000,
        data,
        owner: addr(owner),
        executable: false,
        rent_epoch: 0,
    }
}

pub fn spl_mint(decimals: u8) -> Account {
    token::create_account_for_mint(SplMint {
        decimals,
        is_initialized: true,
        ..SplMint::default()
    })
}

pub fn token_account(mint: &Pubkey, owner: &Pubkey, amount: u64) -> Account {
    token::create_account_for_token_account(SplTokenAccount {
        mint: addr(mint),
        owner: addr(owner),
        amount,
        state: SplAccountState::Initialized,
        ..SplTokenAccount::default()
    })
}

pub fn token_state(account: &Account) -> SplTokenAccount {
    SplTokenAccount::unpack(&account.data)
        .unwrap_or_else(|error| panic!("cannot unpack the token account: {error}"))
}

pub fn keyed_account_for_token_program() -> (Address, Account) {
    token::keyed_account()
}

/// Акаунт самої програми VerdictMesh. Потрібен у списку: CPI бере адресу
/// викликаної програми зі списку акаунтів інструкції, як і будь-який інший
/// ключ.
pub fn keyed_account_for_mesh_program() -> (Address, Account) {
    (
        addr(&MESH_PROGRAM),
        mollusk_svm::program::create_program_account_loader_v3(&addr(&MESH_PROGRAM)),
    )
}

/// Підміна одного акаунта за ключем. Заміна за позицією ламалася б при кожній
/// зміні порядку в `#[derive(Accounts)]` — і мовчки.
pub fn replace(accounts: &mut [(Address, Account)], key: &Pubkey, account: Account) {
    let key = addr(key);
    let entry = accounts
        .iter_mut()
        .find(|(candidate, _)| *candidate == key)
        .unwrap_or_else(|| panic!("{key} is not among the accounts"));
    entry.1 = account;
}

// ── PDA ─────────────────────────────────────────────────────────────────────

pub fn escrow_pda(buyer: &Pubkey, deal_id: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            reference_escrow::seeds::ESCROW,
            buyer.as_ref(),
            &deal_id.to_le_bytes(),
        ],
        &ESCROW_PROGRAM,
    )
}

pub fn escrow_vault_pda(escrow: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[reference_escrow::seeds::ESCROW_VAULT, escrow.as_ref()],
        &ESCROW_PROGRAM,
    )
}

pub fn bond_vault_pda(escrow: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[reference_escrow::seeds::BOND_VAULT, escrow.as_ref()],
        &ESCROW_PROGRAM,
    )
}

pub fn mesh_config_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[verdict_mesh::seeds::CONFIG], &MESH_PROGRAM)
}

pub fn mesh_integrator_pda(authority: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[verdict_mesh::seeds::INTEGRATOR, authority.as_ref()],
        &MESH_PROGRAM,
    )
}

pub fn mesh_dispute_pda(integrator: &Pubkey, dispute_id: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            verdict_mesh::seeds::DISPUTE,
            integrator.as_ref(),
            &dispute_id.to_le_bytes(),
        ],
        &MESH_PROGRAM,
    )
}

pub fn mesh_dispute_vault_pda(dispute: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[verdict_mesh::seeds::DISPUTE_VAULT, dispute.as_ref()],
        &MESH_PROGRAM,
    )
}

// ── готові акаунти VerdictMesh ──────────────────────────────────────────────

/// Демо-політика з docs/PLAN.md — та сама, що в тестах VerdictMesh.
pub fn demo_policy() -> Policy {
    Policy {
        panel_size: 3,
        extended_panel_size: 5,
        quorum: 2,
        extended_quorum: 3,
        juror_stake: usdc(100),
        slash_bps_wrong: 1_000,
        slash_bps_no_reveal: 2_000,
        commit_window: 60,
        reveal_window: 60,
        appeal_window: 90,
        optimistic_window: 60,
        deposit: usdc(5),
        optimistic_threshold: usdc(50),
    }
}

pub fn mesh_config_account(settlement_mint: &Pubkey) -> Account {
    program_account(
        &MESH_PROGRAM,
        &Config {
            settlement_mint: *settlement_mint,
            reporter: Pubkey::new_unique(),
            treasury: Pubkey::new_unique(),
            bump: mesh_config_pda().1,
        },
    )
}

/// Спір у тому вигляді, у якому його лишає `open_dispute`. Повертається сам
/// стан, а не готовий акаунт: тест міняє те поле, заради якого він написаний —
/// вердикт, стан, дедлайн, — і не переносить решту двадцяти щоразу.
pub fn mesh_dispute_state(
    integrator: &Pubkey,
    dispute_id: u64,
    escrow: &Pubkey,
    claimant: &Pubkey,
    respondent: &Pubkey,
    amount: u64,
) -> Dispute {
    let policy = demo_policy();

    Dispute {
        integrator: *integrator,
        dispute_id,
        policy,
        escrow_ref: *escrow,
        claimant: *claimant,
        respondent: *respondent,
        amount,
        state: DisputeState::Tallied,
        panel: Vec::new(),
        report_hash: [0u8; 32],
        claimant_claim_hash: [1u8; 32],
        respondent_claim_hash: [2u8; 32],
        opened_at: NOW,
        entropy_slot: SLOT - 1,
        commit_deadline: NOW + policy.commit_window,
        reveal_deadline: NOW + policy.commit_window + policy.reveal_window,
        appeal_deadline: APPEAL_DEADLINE,
        votes_claimant: 2,
        votes_respondent: 1,
        escalated: false,
        verdict: None,
        bump: 0,
    }
}

/// Вікно апеляції закривається тут — раніше виконувати вердикт не можна.
pub const APPEAL_DEADLINE: i64 = NOW + 10_000;

/// Акаунт спору з місцем під розширену панель — рівно стільки виділяє
/// `open_dispute`.
pub fn mesh_dispute_account(dispute: &Dispute) -> Account {
    let mut account = program_account(&MESH_PROGRAM, dispute);
    account
        .data
        .resize(Dispute::space(dispute.policy.extended_panel_size), 0);
    account
}

/// Mollusk із заданим «зараз». Годинник обв'язки стоїть на `NOW`, і тести вікон
/// мусять рухати його явно, а не покладатись на замовчування.
pub fn mollusk_at(now: i64) -> Mollusk {
    let mut mollusk = mollusk();
    mollusk.sysvars.clock.unix_timestamp = now;
    mollusk
}

/// Зареєстрований інтегратор, чия програма ескроу — саме ця. Такий `Integrator`
/// лишає по собі `register_integrator`.
pub fn mesh_integrator_account(authority: &Pubkey, escrow_program: &Pubkey) -> Account {
    program_account(
        &MESH_PROGRAM,
        &Integrator {
            authority: *authority,
            escrow_program: *escrow_program,
            policy: demo_policy(),
            dispute_count: 0,
            bump: mesh_integrator_pda(authority).1,
        },
    )
}
