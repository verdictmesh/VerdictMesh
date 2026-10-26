//! Test harness for the DePIN escrow, adapted from reference-escrow's.
//!
//! Included from the other test binaries as
//! `#[allow(dead_code)] #[path = "harness.rs"] mod harness;`
//!
//! **Both programs are real.** Opening a dispute is a CPI into VerdictMesh,
//! and whether it accepts the epoch's PDA signature, the policy and the
//! deposit is decided by **that** bytecode, not by a stub.

use anchor_lang::{
    solana_program::pubkey::Pubkey, AccountDeserialize, AnchorDeserialize, AnchorSerialize,
    Discriminator, InstructionData, Space, ToAccountMetas,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use depin_escrow::DepinError;
use mollusk_svm::{program::loader_keys::LOADER_V3, result::InstructionResult, Mollusk};
use mollusk_svm_programs_token::token;
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
use verdict_mesh::state::{Config, Dispute, DisputeState, Integrator, Policy, Verdict};

pub const ESCROW_PROGRAM: Pubkey = depin_escrow::ID;
pub const MESH_PROGRAM: Pubkey = verdict_mesh::ID;

/// mollusk 0.15 speaks the new generation of Solana crates (`Address`),
/// Anchor 0.32.1 the old one (`Pubkey`). The same 32-byte key in two unrelated
/// newtypes; the conversion lives in one place.
pub fn addr(key: &Pubkey) -> Address {
    Address::from(key.to_bytes())
}

pub const DECIMALS: u32 = 6;

pub fn usdc(amount: u64) -> u64 {
    amount * 10u64.pow(DECIMALS)
}

/// A fixed "now" — the same moment as in VerdictMesh's tests.
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
        &elf("depin_escrow.so"),
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

/// Events of one type from the logs, in order of appearance.
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

/// Failure with exactly the expected escrow error, not "some failure". A test
/// that accepts any error stays green when the instruction fails on a missing
/// account instead of the check it claims to prove.
pub fn failed_with(result: &InstructionResult, expected: DepinError) -> bool {
    matches!(&result.raw_result, Err(InstructionError::Custom(code)) if *code == u32::from(expected))
}

/// The same for VerdictMesh's errors: the dispute is opened by another
/// program, and some refusals are its own.
pub fn failed_with_mesh(
    result: &InstructionResult,
    expected: verdict_mesh::VerdictMeshError,
) -> bool {
    matches!(&result.raw_result, Err(InstructionError::Custom(code)) if *code == u32::from(expected))
}

/// The verdict-read helper's errors (T036): the escrow returns them, but they
/// are VerdictMesh's, in a range of their own.
pub fn failed_with_read(
    result: &InstructionResult,
    expected: verdict_mesh::VerdictReadError,
) -> bool {
    matches!(&result.raw_result, Err(InstructionError::Custom(code)) if *code == u32::from(expected))
}

pub const SYSTEM_PROGRAM: Pubkey = anchor_lang::solana_program::system_program::ID;
pub const TOKEN_PROGRAM: Pubkey = anchor_spl::token::ID;

pub fn wallet(lamports: u64) -> Account {
    Account::new(lamports, 0, &addr(&SYSTEM_PROGRAM))
}

/// An account that does not exist yet. Mollusk wants an entry for every key of
/// the instruction, so "no account" has to be passed explicitly as zeroes.
pub fn missing() -> Account {
    Account::default()
}

/// An account the named program once created. Space is allocated for the
/// **full** size of the type: an enum laid out in its shortest variant cannot
/// take a write of a longer one.
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

/// The VerdictMesh program account itself. It has to be in the list: a CPI
/// takes the callee's address from the instruction's accounts like any other
/// key.
pub fn keyed_account_for_mesh_program() -> (Address, Account) {
    (
        addr(&MESH_PROGRAM),
        mollusk_svm::program::create_program_account_loader_v3(&addr(&MESH_PROGRAM)),
    )
}

/// Replaces one account by key. Replacing by position would break — silently —
/// with every reordering in `#[derive(Accounts)]`.
pub fn replace(accounts: &mut [(Address, Account)], key: &Pubkey, account: Account) {
    let key = addr(key);
    let entry = accounts
        .iter_mut()
        .find(|(candidate, _)| *candidate == key)
        .unwrap_or_else(|| panic!("{key} is not among the accounts"));
    entry.1 = account;
}

// ── PDA ─────────────────────────────────────────────────────────────────────

pub fn epoch_pda(network: &Pubkey, epoch_id: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            depin_escrow::seeds::EPOCH,
            network.as_ref(),
            &epoch_id.to_le_bytes(),
        ],
        &ESCROW_PROGRAM,
    )
}

pub fn epoch_vault_pda(epoch: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[depin_escrow::seeds::EPOCH_VAULT, epoch.as_ref()],
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

// ── ready-made VerdictMesh accounts ─────────────────────────────────────────

/// The demo policy from docs/PLAN.md — the same as in VerdictMesh's tests.
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

/// A dispute the way `open_dispute` leaves it. The state is returned rather
/// than an account: a test changes the field it is about — verdict, state,
/// deadline — without restating the other twenty.
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

/// The appeal window closes here — no verdict may be executed earlier.
pub const APPEAL_DEADLINE: i64 = NOW + 10_000;

/// A dispute account with room for the extended panel — exactly what
/// `open_dispute` allocates.
pub fn mesh_dispute_account(dispute: &Dispute) -> Account {
    let mut account = program_account(&MESH_PROGRAM, dispute);
    account
        .data
        .resize(Dispute::space(dispute.policy.extended_panel_size), 0);
    account
}

/// Mollusk at a given "now". The harness clock stands at `NOW`, and window
/// tests must move it explicitly rather than rely on the default.
pub fn mollusk_at(now: i64) -> Mollusk {
    let mut mollusk = mollusk();
    mollusk.sysvars.clock.unix_timestamp = now;
    mollusk
}

/// A registered integrator whose escrow program is this one — what
/// `register_integrator` leaves behind.
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

// ── the epoch fixture ───────────────────────────────────────────────────────

pub const EPOCH_ID: u64 = 3;
pub const WINDOW: i64 = 3_600;
pub const REPORTED_BPS: u16 = 6_000;
pub const CLAIMED_BPS: u16 = 9_000;

pub fn reward() -> u64 {
    usdc(100)
}

/// One epoch between a network and an operator, with everything VerdictMesh
/// needs for a dispute laid out next to it. The reward is in the settlement
/// asset — the escrow insists on it.
pub struct Fixture {
    pub network: Pubkey,
    pub operator: Pubkey,
    pub authority: Pubkey,
    pub mint: Pubkey,
    pub integrator: Pubkey,
    pub epoch: Pubkey,
    pub vault: Pubkey,
    pub network_tokens: Pubkey,
    pub operator_tokens: Pubkey,
    pub accounts: Vec<(Address, Account)>,
}

impl Default for Fixture {
    fn default() -> Self {
        Self::new()
    }
}

impl Fixture {
    pub fn new() -> Self {
        let network = Pubkey::new_unique();
        let operator = Pubkey::new_unique();
        let authority = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        let network_tokens = Pubkey::new_unique();
        let operator_tokens = Pubkey::new_unique();

        let (integrator, _) = mesh_integrator_pda(&authority);
        let (epoch, _) = epoch_pda(&network, EPOCH_ID);
        let (vault, _) = epoch_vault_pda(&epoch);
        let (dispute, _) = mesh_dispute_pda(&integrator, 0);

        let accounts = vec![
            (addr(&network), wallet(10_000_000_000)),
            (addr(&operator), wallet(10_000_000_000)),
            (addr(&mint), spl_mint(DECIMALS as u8)),
            (
                addr(&integrator),
                mesh_integrator_account(&authority, &ESCROW_PROGRAM),
            ),
            (addr(&mesh_config_pda().0), mesh_config_account(&mint)),
            (addr(&epoch), missing()),
            (addr(&vault), missing()),
            (
                addr(&network_tokens),
                token_account(&mint, &network, usdc(500)),
            ),
            (
                addr(&operator_tokens),
                token_account(&mint, &operator, usdc(50)),
            ),
            (addr(&dispute), missing()),
            (addr(&mesh_dispute_vault_pda(&dispute).0), missing()),
            keyed_account_for_token_program(),
            keyed_account_for_mesh_program(),
            mollusk_svm::program::keyed_account_for_system_program(),
        ];

        Self {
            network,
            operator,
            authority,
            mint,
            integrator,
            epoch,
            vault,
            network_tokens,
            operator_tokens,
            accounts,
        }
    }

    pub fn dispute(&self) -> Pubkey {
        mesh_dispute_pda(&self.integrator, 0).0
    }

    pub fn fund_ix(&self, reward: u64, challenge_window: i64) -> Instruction {
        self.fund_ix_with(self.integrator, self.mint, reward, challenge_window)
    }

    pub fn fund_ix_with(
        &self,
        integrator: Pubkey,
        mint: Pubkey,
        reward: u64,
        challenge_window: i64,
    ) -> Instruction {
        anchor_ix(
            &ESCROW_PROGRAM,
            depin_escrow::accounts::FundEpoch {
                network: self.network,
                operator: self.operator,
                integrator,
                config: mesh_config_pda().0,
                mint,
                epoch: self.epoch,
                network_tokens: self.network_tokens,
                vault: self.vault,
                token_program: TOKEN_PROGRAM,
                system_program: SYSTEM_PROGRAM,
            },
            depin_escrow::instruction::FundEpoch {
                epoch_id: EPOCH_ID,
                reward,
                challenge_window,
            },
        )
    }

    pub fn report_ix(&self, operator_bps: u16) -> Instruction {
        self.report_ix_by(self.network, operator_bps)
    }

    pub fn report_ix_by(&self, network: Pubkey, operator_bps: u16) -> Instruction {
        anchor_ix(
            &ESCROW_PROGRAM,
            depin_escrow::accounts::ReportUsage {
                network,
                epoch: self.epoch,
            },
            depin_escrow::instruction::ReportUsage { operator_bps },
        )
    }

    pub fn settle_report_ix(&self) -> Instruction {
        anchor_ix(
            &ESCROW_PROGRAM,
            depin_escrow::accounts::SettleReport {
                epoch: self.epoch,
                mint: self.mint,
                network_tokens: self.network_tokens,
                operator_tokens: self.operator_tokens,
                vault: self.vault,
                token_program: TOKEN_PROGRAM,
            },
            depin_escrow::instruction::SettleReport {},
        )
    }

    /// The names here are the SDK's `DisputeAccounts` (`prepareDispute`): if
    /// one of them drifts, this stops compiling.
    pub fn dispute_ix(&self, claimed_bps: u16) -> Instruction {
        self.dispute_ix_with(self.operator, self.integrator, claimed_bps)
    }

    pub fn dispute_ix_with(
        &self,
        operator: Pubkey,
        integrator: Pubkey,
        claimed_bps: u16,
    ) -> Instruction {
        let dispute = self.dispute();
        anchor_ix(
            &ESCROW_PROGRAM,
            depin_escrow::accounts::DisputeEpoch {
                operator,
                epoch: self.epoch,
                integrator,
                config: mesh_config_pda().0,
                settlement_mint: self.mint,
                dispute,
                claimant_tokens: self.operator_tokens,
                dispute_vault: mesh_dispute_vault_pda(&dispute).0,
                verdict_mesh_program: MESH_PROGRAM,
                token_program: TOKEN_PROGRAM,
                system_program: SYSTEM_PROGRAM,
            },
            depin_escrow::instruction::DisputeEpoch { claimed_bps },
        )
    }

    pub fn settle_dispute_ix(&self, dispute: Pubkey) -> Instruction {
        anchor_ix(
            &ESCROW_PROGRAM,
            depin_escrow::accounts::SettleDispute {
                epoch: self.epoch,
                dispute,
                mint: self.mint,
                network_tokens: self.network_tokens,
                operator_tokens: self.operator_tokens,
                vault: self.vault,
                token_program: TOKEN_PROGRAM,
            },
            depin_escrow::instruction::SettleDispute {},
        )
    }

    /// Runs `ix` at `now`, asserts success and carries the changed accounts
    /// over — so a test of a later step does not repeat the earlier ones.
    pub fn after(
        &self,
        accounts: &[(Address, Account)],
        now: i64,
        ix: &Instruction,
    ) -> Vec<(Address, Account)> {
        let result = mollusk_at(now).process_instruction(ix, accounts);
        assert!(result.program_result.is_ok(), "{:?}", result.raw_result);
        let mut next = accounts.to_vec();
        for (key, account) in &result.resulting_accounts {
            if let Some(entry) = next.iter_mut().find(|(candidate, _)| candidate == key) {
                entry.1 = account.clone();
            }
        }
        next
    }

    pub fn funded(&self) -> Vec<(Address, Account)> {
        self.after(&self.accounts, NOW, &self.fund_ix(reward(), WINDOW))
    }

    /// Funded, and reported at `NOW`: the window closes at `NOW + WINDOW`.
    pub fn reported(&self) -> Vec<(Address, Account)> {
        self.after(&self.funded(), NOW, &self.report_ix(REPORTED_BPS))
    }

    /// Reported, then challenged for `CLAIMED_BPS` through the real CPI.
    pub fn disputed(&self) -> Vec<(Address, Account)> {
        self.after(&self.reported(), NOW + 1, &self.dispute_ix(CLAIMED_BPS))
    }

    pub fn epoch_state(&self, accounts: &[(Address, Account)]) -> depin_escrow::Epoch {
        decode(find(accounts, &self.epoch))
    }

    pub fn balance(&self, accounts: &[(Address, Account)], key: &Pubkey) -> u64 {
        token_state(find(accounts, key)).amount
    }

    /// The dispute VerdictMesh opened, brought to the end of review: `verdict`,
    /// appeal window closing at `APPEAL_DEADLINE`. Everything else as opened —
    /// the escrow is tested against the account the real CPI wrote.
    pub fn ruled(
        &self,
        accounts: &[(Address, Account)],
        verdict: Option<Verdict>,
    ) -> Vec<(Address, Account)> {
        let key = self.dispute();
        let mut dispute: Dispute = decode(find(accounts, &key));
        dispute.verdict = verdict;
        dispute.appeal_deadline = APPEAL_DEADLINE;
        let mut next = accounts.to_vec();
        replace(&mut next, &key, mesh_dispute_account(&dispute));
        next
    }
}

pub fn find<'a>(accounts: &'a [(Address, Account)], key: &Pubkey) -> &'a Account {
    let key = addr(key);
    accounts
        .iter()
        .find(|(candidate, _)| *candidate == key)
        .map(|(_, account)| account)
        .unwrap_or_else(|| panic!("{key} is not among the accounts"))
}
