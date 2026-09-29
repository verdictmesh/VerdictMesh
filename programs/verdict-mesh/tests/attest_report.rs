//! T030 — the report fingerprint on chain (`FR-017`, `FR-017a`).
//!
//! `attest_report` is the only instruction the reporter role can sign, and the
//! role is the only privileged key in the protocol. Two promises are checked
//! here, and both fail quietly when broken.
//!
//! **Only the reporter, only once.** A fingerprint anyone could write is not a
//! fingerprint of *our* report, and one that can be rewritten lets the report
//! change after jurors have read it — the substitution `FR-017b` is meant to
//! expose would then match the chain.
//!
//! **Nothing but the fingerprint.** The role must not be able to vote, change
//! a verdict, or move escrow funds, stakes or deposits (`FR-017a`). The first
//! half of this file shows the instruction touches one field; the second half
//! throws the reporter key at every other door and checks each one stays shut.

#[allow(dead_code)]
#[path = "harness.rs"]
mod harness;

use anchor_lang::{error::ErrorCode, solana_program::pubkey::Pubkey};
use harness::*;
use mollusk_svm::{program::keyed_account_for_system_program, result::InstructionResult};
use solana_account::Account;
use solana_address::Address;
use solana_instruction::Instruction;
use verdict_mesh::{
    events::ReportAttested,
    state::{Config, Dispute, DisputeState, Integrator, Juror, JurorIndex, JurorRegistry, Verdict},
    VerdictMeshError,
};

const DISPUTE_ID: u64 = 0;
const PANEL: usize = 3;

/// The fingerprint the reporter writes — any non-zero 32 bytes will do: the
/// program cannot recompute a hash of a report it never sees.
const REPORT: [u8; 32] = [0xAB; 32];
const OTHER_REPORT: [u8; 32] = [0xCD; 32];

/// A dispute that has just been opened and drawn a panel, and the protocol
/// config that names the reporter.
struct Fixture {
    reporter: Pubkey,
    authority: Pubkey,
    dispute: Pubkey,
    opened: Dispute,
    panel: Vec<Pubkey>,
    mint: Pubkey,
    treasury: Pubkey,
    accounts: Vec<(Address, Account)>,
}

impl Fixture {
    fn new() -> Self {
        Self::with(|_| {})
    }

    /// The dispute is built from state, not by running `open_dispute` and
    /// `select_panel`: a test about the fingerprint must not fail on the draw.
    fn with(change: impl FnOnce(&mut Dispute)) -> Self {
        let reporter = Pubkey::new_unique();
        let authority = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        let treasury = Pubkey::new_unique();
        let (integrator, _) = integrator_pda(&authority);
        let (dispute, bump) = dispute_pda(&integrator, DISPUTE_ID);

        let panel: Vec<Pubkey> = (0..PANEL).map(|_| Pubkey::new_unique()).collect();

        let mut opened = dispute_state(&integrator, DISPUTE_ID, &demo_policy(), bump);
        opened.panel = panel.clone();
        change(&mut opened);

        let mut accounts = vec![
            (addr(&reporter), wallet(10_000_000_000)),
            (addr(&authority), wallet(10_000_000_000)),
            (
                addr(&config_pda().0),
                config_account(&mint, &reporter, &treasury),
            ),
            (addr(&dispute), dispute_account(&opened)),
            (addr(&vote_pda(&dispute, &reporter).0), missing()),
            keyed_account_for_system_program(),
        ];
        for juror in &panel {
            accounts.push((addr(juror), wallet(10_000_000_000)));
        }

        Self {
            reporter,
            authority,
            dispute,
            opened,
            panel,
            mint,
            treasury,
            accounts,
        }
    }

    fn ix(&self, signer: &Pubkey, report_hash: [u8; 32]) -> Instruction {
        self.ix_with_config(signer, &config_pda().0, report_hash)
    }

    fn ix_with_config(
        &self,
        signer: &Pubkey,
        config: &Pubkey,
        report_hash: [u8; 32],
    ) -> Instruction {
        anchor_ix(
            verdict_mesh::accounts::AttestReport {
                reporter: *signer,
                config: *config,
                dispute: self.dispute,
            },
            verdict_mesh::instruction::AttestReport { report_hash },
        )
    }

    fn attest(&self, report_hash: [u8; 32]) -> InstructionResult {
        self.attest_at(report_hash, NOW)
    }

    fn attest_at(&self, report_hash: [u8; 32], now: i64) -> InstructionResult {
        mollusk_at(now).process_instruction(&self.ix(&self.reporter, report_hash), &self.accounts)
    }


    fn deadline(&self) -> i64 {
        self.opened.commit_deadline
    }

    /// The dispute as the chain holds it after `result`.
    fn dispute_after(&self, result: &InstructionResult) -> Dispute {
        decode(resulting(result, &self.dispute))
    }

    /// The same fixture after a successful attestation: the next attempt runs
    /// against the state the first one left behind.
    fn after(mut self, result: &InstructionResult) -> Self {
        let written = resulting(result, &self.dispute).clone();
        replace(&mut self.accounts, &self.dispute, written);
        self
    }
}

// ── the fingerprint ─────────────────────────────────────────────────────────

#[test]
fn writes_the_fingerprint_the_reporter_signed() {
    let fixture = Fixture::new();

    let result = fixture.attest(REPORT);

    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);
    assert_eq!(fixture.dispute_after(&result).report_hash, REPORT);
}

/// Everything except `report_hash` is left byte for byte as it was: state,
/// panel, deadlines, counters, verdict. This is the behavioural half of
/// `FR-017a` for the instruction itself — whatever the fingerprint says, it
/// cannot be anything but a fingerprint.
#[test]
fn changes_nothing_in_the_dispute_but_the_fingerprint() {
    let fixture = Fixture::new();

    let result = fixture.attest(REPORT);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let mut expected = fixture.opened.clone();
    expected.report_hash = REPORT;
    let before = dispute_account(&fixture.opened);
    let after = resulting(&result, &fixture.dispute);

    assert_eq!(after.data, dispute_account(&expected).data);
    assert_eq!(after.lamports, before.lamports);
    assert_eq!(after.owner, before.owner);
}

/// `FR-029`: an outside observer sees when the fingerprint appeared and what
/// it was, and can check a published report against it without our service.
#[test]
fn announces_the_fingerprint_it_wrote() {
    let fixture = Fixture::new();
    let (mollusk, logs) = mollusk_with_logs();

    let result =
        mollusk.process_instruction(&fixture.ix(&fixture.reporter, REPORT), &fixture.accounts);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let events = emitted::<ReportAttested>(&logs);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].dispute, fixture.dispute);
    assert_eq!(events[0].report_hash, REPORT);
}

/// Up to the last second of the commit window the fingerprint is still in
/// time; the window is the one jurors seal their votes in.
#[test]
fn accepts_the_fingerprint_in_the_last_second_of_the_commit_window() {
    let fixture = Fixture::new();

    let result = fixture.attest_at(REPORT, fixture.deadline() - 1);

    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);
}

// ── only the reporter ───────────────────────────────────────────────────────

/// Neither side of the dispute, nor its integrator, nor a juror can write the
/// fingerprint. Each of them has a reason to want a report of their own on
/// chain, and the error names the missing role rather than a generic failure.
#[test]
fn refuses_a_fingerprint_from_anyone_but_the_reporter() {
    let fixture = Fixture::new();

    let strangers = [
        ("claimant", fixture.opened.claimant),
        ("respondent", fixture.opened.respondent),
        ("integrator authority", fixture.authority),
        ("juror", fixture.panel[0]),
        ("stranger", Pubkey::new_unique()),
    ];

    for (who, signer) in strangers {
        let mut accounts = fixture.accounts.clone();
        if !accounts.iter().any(|(key, _)| *key == addr(&signer)) {
            accounts.push((addr(&signer), wallet(10_000_000_000)));
        }

        let result = mollusk().process_instruction(&fixture.ix(&signer, REPORT), &accounts);

        assert!(
            failed_with(&result, VerdictMeshError::NotReporter),
            "{who}: {:?}",
            result.raw_result
        );
    }
}

/// The reporter's key named without its signature. The role is a key, not an
/// address: whoever knows the address knows it from `Config`.
#[test]
fn refuses_the_reporter_key_without_its_signature() {
    let fixture = Fixture::new();

    let mut ix = fixture.ix(&fixture.reporter, REPORT);
    let reporter = addr(&fixture.reporter);
    for meta in ix.accounts.iter_mut() {
        if meta.pubkey == reporter {
            meta.is_signer = false;
        }
    }

    let result = mollusk().process_instruction(&ix, &fixture.accounts);
    assert!(
        failed_with_anchor(&result, ErrorCode::AccountNotSigner),
        "{:?}",
        result.raw_result
    );
}

/// A config of one's own making. Anything owned by the program and shaped like
/// `Config` names whatever reporter it likes, so the role is read only from the
/// single address `initialize` wrote it to.
#[test]
fn refuses_a_config_that_is_not_the_protocol_config() {
    let fixture = Fixture::new();
    let impostor = Pubkey::new_unique();
    let forged = Pubkey::new_unique();

    let mut accounts = fixture.accounts.clone();
    accounts.push((addr(&impostor), wallet(10_000_000_000)));
    accounts.push((
        addr(&forged),
        program_account(&Config {
            settlement_mint: fixture.mint,
            reporter: impostor,
            treasury: fixture.treasury,
            bump: config_pda().1,
        }),
    ));

    let ix = fixture.ix_with_config(&impostor, &forged, REPORT);
    let result = mollusk().process_instruction(&ix, &accounts);

    assert!(
        failed_with_anchor(&result, ErrorCode::ConstraintSeeds),
        "{:?}",
        result.raw_result
    );
}

// ── only once ───────────────────────────────────────────────────────────────

/// A second fingerprint is refused whatever it says — a different report is a
/// substitution, and the same one is a transaction with nothing to do.
#[test]
fn refuses_to_write_a_second_fingerprint() {
    let first = Fixture::new();
    let result = first.attest(REPORT);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);
    let attested = first.after(&result);

    for second in [OTHER_REPORT, REPORT] {
        let result = attested.attest(second);

        assert!(
            failed_with(&result, VerdictMeshError::ReportAlreadyAttested),
            "{:?}",
            result.raw_result
        );
    }
}

/// And the refusal leaves the first one in place.
#[test]
fn keeps_the_first_fingerprint_after_a_second_attempt() {
    let first = Fixture::new();
    let result = first.attest(REPORT);
    let attested = first.after(&result);

    let result = attested.attest(OTHER_REPORT);

    assert!(result.program_result.is_err());
    assert_eq!(attested.dispute_after(&result).report_hash, REPORT);
}

/// Zero is what the field holds before any report — "not attested". Writing
/// it would record nothing and leave the door open for a second attestation,
/// which is exactly what "only once" closes.
#[test]
fn refuses_an_empty_fingerprint() {
    let fixture = Fixture::new();

    let result = fixture.attest([0u8; 32]);

    assert!(
        failed_with(&result, VerdictMeshError::EmptyReportHash),
        "{:?}",
        result.raw_result
    );
}

// ── only before the votes are sealed ────────────────────────────────────────

/// `FR-017`: the report is fixed before jurors commit. A fingerprint arriving
/// after the commit window would describe a report written with the sealed
/// votes already on chain.
#[test]
fn refuses_a_fingerprint_once_the_commit_window_has_closed() {
    let fixture = Fixture::new();

    for now in [fixture.deadline(), fixture.deadline() + 1] {
        let result = fixture.attest_at(REPORT, now);

        assert!(
            failed_with(&result, VerdictMeshError::WindowClosed),
            "at {now}: {:?}",
            result.raw_result
        );
    }
}

/// Past the commit stage the dispute no longer takes a report, even with a
/// clock that says otherwise — the state, not the time, is what jurors act on.
#[test]
fn refuses_a_fingerprint_outside_the_commit_stage() {
    for state in [
        DisputeState::OptimisticPending,
        DisputeState::Revealing,
        DisputeState::Tallied,
        DisputeState::Appealed,
        DisputeState::Finalized,
    ] {
        let fixture = Fixture::with(|dispute| dispute.state = state);

        let result = fixture.attest(REPORT);

        assert!(
            failed_with(&result, VerdictMeshError::WrongState),
            "{state:?}: {:?}",
            result.raw_result
        );
    }
}

/// After escalation the dispute is back in `Committing` with a fresh window,
/// but the first round's votes are already revealed. A report written now would
/// be written knowing them — and read by the extended panel as if it were not.
#[test]
fn refuses_a_fingerprint_in_the_escalated_round() {
    let fixture = Fixture::with(|dispute| {
        dispute.escalated = true;
        dispute.votes_claimant = 1;
        dispute.votes_respondent = 1;
    });

    let result = fixture.attest(REPORT);

    assert!(
        failed_with(&result, VerdictMeshError::WrongState),
        "{:?}",
        result.raw_result
    );
}

// ── FR-017a: the reporter key at every other door ───────────────────────────
//
// Each test below signs with the reporter key an instruction that would give
// it a say in a dispute or access to money, on a live dispute where the key is
// the configured reporter. Each attempt must fail on the check that protects
// that door — not on some unrelated missing account, which would prove
// nothing about the role.

/// Voting. The reporter is not on the panel, and a sealed vote from outside
/// the panel is refused before it can ever be counted.
#[test]
fn the_reporter_cannot_vote() {
    let fixture = Fixture::new();

    let ix = anchor_ix(
        verdict_mesh::accounts::CommitVote {
            juror: fixture.reporter,
            dispute: fixture.dispute,
            vote: vote_pda(&fixture.dispute, &fixture.reporter).0,
            system_program: SYSTEM_PROGRAM,
        },
        verdict_mesh::instruction::CommitVote {
            commitment: [7u8; 32],
        },
    );

    let result = mollusk().process_instruction(&ix, &fixture.accounts);
    assert!(
        failed_with(&result, VerdictMeshError::NotOnPanel),
        "{:?}",
        result.raw_result
    );
}

/// Escrow funds. The program never moves them — the escrow pulls the verdict
/// and pays out itself — so the only lever over an escrow is putting its
/// balance under a dispute. That needs the escrow's own signature, and the
/// reporter signing in its place is refused on ownership.
#[test]
fn the_reporter_cannot_put_an_escrow_under_dispute() {
    let fixture = Fixture::new();
    let escrow_program = Pubkey::new_unique();
    let (integrator, integrator_bump) = integrator_pda(&fixture.authority);
    let dispute = dispute_pda(&integrator, DISPUTE_ID).0;
    let reporter_tokens = Pubkey::new_unique();

    let accounts = vec![
        (addr(&fixture.reporter), wallet(10_000_000_000)),
        (
            addr(&config_pda().0),
            config_account(&fixture.mint, &fixture.reporter, &fixture.treasury),
        ),
        (addr(&fixture.mint), settlement_mint()),
        (
            addr(&integrator),
            program_account(&Integrator {
                authority: fixture.authority,
                escrow_program,
                policy: demo_policy(),
                dispute_count: DISPUTE_ID,
                bump: integrator_bump,
            }),
        ),
        (addr(&dispute), missing()),
        (
            addr(&reporter_tokens),
            token_account(&fixture.mint, &fixture.reporter, usdc(100)),
        ),
        (addr(&dispute_vault_pda(&dispute).0), missing()),
        keyed_account_for_token_program(),
        keyed_account_for_system_program(),
    ];

    let ix = anchor_ix(
        verdict_mesh::accounts::OpenDispute {
            payer: fixture.reporter,
            config: config_pda().0,
            settlement_mint: fixture.mint,
            depositor: fixture.reporter,
            integrator,
            escrow: fixture.reporter,
            dispute,
            depositor_tokens: reporter_tokens,
            dispute_vault: dispute_vault_pda(&dispute).0,
            token_program: TOKEN_PROGRAM,
            system_program: SYSTEM_PROGRAM,
        },
        verdict_mesh::instruction::OpenDispute {
            claimant: fixture.reporter,
            respondent: Pubkey::new_unique(),
            amount: usdc(10),
            claimant_claim_hash: [1u8; 32],
            respondent_claim_hash: [2u8; 32],
        },
    );

    let result = mollusk().process_instruction(&ix, &accounts);
    assert!(
        failed_with_anchor(&result, ErrorCode::ConstraintOwner),
        "{:?}",
        result.raw_result
    );
}

/// Stakes. A juror's stake leaves the vault only for the juror's own tokens and
/// only by the juror's signature: the juror record is derived from the signer,
/// so the reporter signing for someone else's record does not reach it.
#[test]
fn the_reporter_cannot_withdraw_a_jurors_stake() {
    let fixture = Fixture::new();
    let juror = fixture.panel[0];
    let reporter_tokens = Pubkey::new_unique();
    let (registry, registry_bump) = registry_pda();

    let accounts = vec![
        (addr(&fixture.reporter), wallet(10_000_000_000)),
        (
            addr(&config_pda().0),
            config_account(&fixture.mint, &fixture.reporter, &fixture.treasury),
        ),
        (addr(&fixture.mint), settlement_mint()),
        (
            addr(&registry),
            program_account(&JurorRegistry {
                juror_count: 1,
                bump: registry_bump,
            }),
        ),
        (
            addr(&juror_pda(&juror).0),
            program_account(&Juror {
                wallet: juror,
                stake: usdc(100),
                active_disputes: 0,
                index: 0,
                bump: juror_pda(&juror).1,
            }),
        ),
        (
            addr(&juror_index_pda(0).0),
            program_account(&JurorIndex {
                wallet: juror,
                bump: juror_index_pda(0).1,
            }),
        ),
        (
            addr(&reporter_tokens),
            token_account(&fixture.mint, &fixture.reporter, 0),
        ),
        (
            addr(&stake_vault_pda().0),
            vault_account(&fixture.mint, usdc(100)),
        ),
        keyed_account_for_token_program(),
        keyed_account_for_system_program(),
        keyed_account_for_this_program(),
    ];

    let ix = anchor_ix(
        verdict_mesh::accounts::Unstake {
            juror: fixture.reporter,
            config: config_pda().0,
            settlement_mint: fixture.mint,
            registry,
            juror_account: juror_pda(&juror).0,
            tail_index: juror_index_pda(0).0,
            vacated_index: None,
            mover: None,
            juror_tokens: reporter_tokens,
            stake_vault: stake_vault_pda().0,
            token_program: TOKEN_PROGRAM,
            system_program: SYSTEM_PROGRAM,
        },
        verdict_mesh::instruction::Unstake {},
    );

    let result = mollusk().process_instruction(&ix, &accounts);
    assert!(
        failed_with_anchor(&result, ErrorCode::ConstraintSeeds),
        "{:?}",
        result.raw_result
    );
}

/// Deposits and slashing. `settle_stakes` is the one instruction that moves
/// the dispute's deposit and slashes stakes, and anyone may crank it — so the
/// reporter may too. What it cannot do is crank it before there is a verdict:
/// settlement is a function of the tally, not of who asks for it.
#[test]
fn the_reporter_cannot_settle_a_dispute_that_has_no_verdict() {
    let fixture = Fixture::new();
    let treasury_tokens = Pubkey::new_unique();

    let mut accounts = fixture.accounts.clone();
    accounts.extend([
        (addr(&fixture.mint), settlement_mint()),
        (
            addr(&dispute_vault_pda(&fixture.dispute).0),
            vault_account(&fixture.mint, demo_policy().deposit),
        ),
        (
            addr(&stake_vault_pda().0),
            vault_account(&fixture.mint, usdc(300)),
        ),
        (
            addr(&treasury_tokens),
            token_account(&fixture.mint, &fixture.treasury, 0),
        ),
        keyed_account_for_token_program(),
    ]);

    let ix = anchor_ix(
        verdict_mesh::accounts::SettleStakes {
            dispute: fixture.dispute,
            crank: fixture.reporter,
            config: config_pda().0,
            settlement_mint: fixture.mint,
            dispute_vault: dispute_vault_pda(&fixture.dispute).0,
            stake_vault: stake_vault_pda().0,
            treasury_tokens,
            token_program: TOKEN_PROGRAM,
        },
        verdict_mesh::instruction::SettleStakes {},
    );

    let result = mollusk().process_instruction(&ix, &accounts);
    assert!(
        failed_with(&result, VerdictMeshError::WrongState),
        "{:?}",
        result.raw_result
    );
}

/// The verdict. `tally` takes no signature, so the reporter has nothing to
/// add there; the only instruction the key can sign is this one, and on a
/// decided dispute it is refused and leaves the verdict where it was.
/// `authority.rs` lists every signature the program asks for.
#[test]
fn the_reporter_cannot_touch_a_decided_verdict() {
    let fixture = Fixture::with(|dispute| {
        dispute.state = DisputeState::Tallied;
        dispute.verdict = Some(Verdict::Claimant);
        dispute.votes_claimant = 2;
        dispute.votes_respondent = 1;
    });

    let result = fixture.attest(REPORT);

    assert!(result.program_result.is_err());
    let after = fixture.dispute_after(&result);
    assert_eq!(after.verdict, Some(Verdict::Claimant));
    assert_eq!(after.report_hash, [0u8; 32]);
}
