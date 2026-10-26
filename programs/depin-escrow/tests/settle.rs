//! Executing a verdict over an epoch — `FR-012`, `FR-013`, `FR-026a`,
//! `FR-026f`, `FR-027a`.
//!
//! Every test starts from the dispute the real `open_dispute` CPI wrote, and
//! only moves it to the end of review (verdict, appeal window): the escrow is
//! checked against the account VerdictMesh actually produces.

#[allow(dead_code)]
#[path = "harness.rs"]
mod harness;

use anchor_lang::solana_program::pubkey::Pubkey;
use depin_escrow::{events::EpochSettled, DepinError, EpochState};
use harness::*;
use solana_account::Account;
use solana_address::Address;
use verdict_mesh::{
    state::{Dispute, DisputeState, Verdict},
    VerdictReadError,
};

fn deposit() -> u64 {
    demo_policy().deposit
}

/// The dispute in `accounts`, changed by `change`.
fn with_dispute(
    fixture: &Fixture,
    accounts: &[(Address, Account)],
    change: impl FnOnce(&mut Dispute),
) -> Vec<(Address, Account)> {
    let mut dispute: Dispute = decode(find(accounts, &fixture.dispute()));
    change(&mut dispute);
    let mut next = accounts.to_vec();
    replace(
        &mut next,
        &fixture.dispute(),
        mesh_dispute_account(&dispute),
    );
    next
}

fn settle(fixture: &Fixture, accounts: &[(Address, Account)]) -> Vec<(Address, Account)> {
    fixture.after(
        accounts,
        APPEAL_DEADLINE,
        &fixture.settle_dispute_ix(fixture.dispute()),
    )
}

// ── who gets what ───────────────────────────────────────────────────────────

/// The operator's offer stands, and the network — against whom the verdict
/// went — reimburses the deposit out of its own share (`FR-026a`).
#[test]
fn pays_the_operator_s_offer_and_reimburses_the_deposit_when_the_operator_wins() {
    let fixture = Fixture::new();
    let accounts = settle(
        &fixture,
        &fixture.ruled(&fixture.disputed(), Some(Verdict::Claimant)),
    );

    assert_eq!(
        fixture.balance(&accounts, &fixture.operator_tokens),
        usdc(50) - deposit() + usdc(90) + deposit()
    );
    assert_eq!(
        fixture.balance(&accounts, &fixture.network_tokens),
        usdc(500) - reward() + usdc(10) - deposit()
    );
    assert_eq!(fixture.balance(&accounts, &fixture.vault), 0);
    assert_eq!(
        fixture.epoch_state(&accounts).state,
        EpochState::Settled {
            operator_bps: CLAIMED_BPS
        }
    );
}

/// The network's report stands; the operator's deposit has paid for the
/// review, and nobody pays twice.
#[test]
fn pays_the_report_when_the_network_wins() {
    let fixture = Fixture::new();
    let accounts = settle(
        &fixture,
        &fixture.ruled(&fixture.disputed(), Some(Verdict::Respondent)),
    );

    assert_eq!(
        fixture.balance(&accounts, &fixture.operator_tokens),
        usdc(50) - deposit() + usdc(60)
    );
    assert_eq!(
        fixture.balance(&accounts, &fixture.network_tokens),
        usdc(500) - reward() + usdc(40)
    );
    assert_eq!(
        fixture.epoch_state(&accounts).state,
        EpochState::Settled {
            operator_bps: REPORTED_BPS
        }
    );
}

/// `FR-027a`: "as if there had been no dispute" — and without one the report
/// would have been paid. The epoch closes; it does not reopen for another
/// round.
#[test]
fn pays_the_report_and_closes_the_epoch_on_status_quo() {
    let fixture = Fixture::new();
    let accounts = settle(
        &fixture,
        &fixture.ruled(&fixture.disputed(), Some(Verdict::StatusQuo)),
    );

    assert_eq!(
        fixture.balance(&accounts, &fixture.operator_tokens),
        usdc(50) - deposit() + usdc(60)
    );
    assert_eq!(
        fixture.balance(&accounts, &fixture.network_tokens),
        usdc(500) - reward() + usdc(40)
    );
    assert_eq!(
        fixture.epoch_state(&accounts).state,
        EpochState::Settled {
            operator_bps: REPORTED_BPS
        }
    );
}

/// `FR-026f`: the network's share is all there is to reimburse from. At 98 %
/// it holds 2 of the 5 — the operator gets those, the rest is charged to
/// nobody, and the operator's share under the verdict is not cut.
#[test]
fn reimburses_no_more_than_the_network_share() {
    let fixture = Fixture::new();
    let disputed = fixture.after(&fixture.reported(), NOW + 1, &fixture.dispute_ix(9_800));
    let accounts = settle(&fixture, &fixture.ruled(&disputed, Some(Verdict::Claimant)));

    assert_eq!(
        fixture.balance(&accounts, &fixture.operator_tokens),
        usdc(50) - deposit() + usdc(98) + usdc(2)
    );
    assert_eq!(
        fixture.balance(&accounts, &fixture.network_tokens),
        usdc(500) - reward()
    );
    assert_eq!(fixture.balance(&accounts, &fixture.vault), 0);
}

#[test]
fn emits_the_settlement_event_with_the_reimbursement() {
    let fixture = Fixture::new();
    let accounts = fixture.ruled(&fixture.disputed(), Some(Verdict::Claimant));
    let (mut mollusk, logs) = mollusk_with_logs();
    mollusk.sysvars.clock.unix_timestamp = APPEAL_DEADLINE;
    let result =
        mollusk.process_instruction(&fixture.settle_dispute_ix(fixture.dispute()), &accounts);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let events = emitted::<EpochSettled>(&logs);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].dispute, Some(fixture.dispute()));
    assert_eq!(events[0].operator_bps, CLAIMED_BPS);
    assert_eq!(events[0].operator_amount, usdc(90) + deposit());
    assert_eq!(events[0].network_amount, usdc(10) - deposit());
    assert_eq!(events[0].reimbursed, deposit());
}

// ── when not to pay ─────────────────────────────────────────────────────────

/// `FR-013`: after the payout the epoch no longer names the dispute.
#[test]
fn executes_a_verdict_once() {
    let fixture = Fixture::new();
    let settled = settle(
        &fixture,
        &fixture.ruled(&fixture.disputed(), Some(Verdict::Claimant)),
    );
    let result = mollusk_at(APPEAL_DEADLINE)
        .process_instruction(&fixture.settle_dispute_ix(fixture.dispute()), &settled);
    assert!(failed_with(&result, DepinError::NotUnderThisDispute));
}

#[test]
fn waits_for_the_verdict() {
    let fixture = Fixture::new();
    let accounts = fixture.ruled(&fixture.disputed(), None);
    let result = mollusk_at(APPEAL_DEADLINE)
        .process_instruction(&fixture.settle_dispute_ix(fixture.dispute()), &accounts);
    assert!(failed_with_read(&result, VerdictReadError::VerdictPending));
}

#[test]
fn waits_for_the_appeal_window() {
    let fixture = Fixture::new();
    let accounts = fixture.ruled(&fixture.disputed(), Some(Verdict::Claimant));
    let result = mollusk_at(APPEAL_DEADLINE - 1)
        .process_instruction(&fixture.settle_dispute_ix(fixture.dispute()), &accounts);
    assert!(failed_with_read(
        &result,
        VerdictReadError::AppealWindowOpen
    ));
}

#[test]
fn holds_a_verdict_under_appeal() {
    let fixture = Fixture::new();
    let ruled = fixture.ruled(&fixture.disputed(), Some(Verdict::Claimant));
    let accounts = with_dispute(&fixture, &ruled, |dispute| {
        dispute.state = DisputeState::Appealed;
    });
    let result = mollusk_at(APPEAL_DEADLINE)
        .process_instruction(&fixture.settle_dispute_ix(fixture.dispute()), &accounts);
    assert!(failed_with_read(
        &result,
        VerdictReadError::VerdictUnderAppeal
    ));
}

#[test]
fn refuses_a_dispute_over_another_escrow() {
    let fixture = Fixture::new();
    let ruled = fixture.ruled(&fixture.disputed(), Some(Verdict::Claimant));
    let accounts = with_dispute(&fixture, &ruled, |dispute| {
        dispute.escrow_ref = Pubkey::new_unique();
    });
    let result = mollusk_at(APPEAL_DEADLINE)
        .process_instruction(&fixture.settle_dispute_ix(fixture.dispute()), &accounts);
    assert!(failed_with_read(&result, VerdictReadError::NotOurDispute));
}

/// A genuine VerdictMesh dispute that names this epoch but is not the one
/// recorded in it — the check `Dispute::ruling` cannot make.
#[test]
fn refuses_a_dispute_the_epoch_does_not_record() {
    let fixture = Fixture::new();
    let (other, _) = mesh_dispute_pda(&fixture.integrator, 1);
    let mut state = mesh_dispute_state(
        &fixture.integrator,
        1,
        &fixture.epoch,
        &fixture.operator,
        &fixture.network,
        usdc(40),
    );
    state.verdict = Some(Verdict::Claimant);

    let mut accounts = fixture.disputed();
    accounts.push((addr(&other), mesh_dispute_account(&state)));

    let result = mollusk_at(APPEAL_DEADLINE)
        .process_instruction(&fixture.settle_dispute_ix(other), &accounts);
    assert!(failed_with(&result, DepinError::NotUnderThisDispute));
}

/// A `Dispute` laid out by some other program cannot write itself a verdict:
/// the account is typed, so Anchor checks its owner.
#[test]
fn refuses_a_dispute_owned_by_another_program() {
    let fixture = Fixture::new();
    let ruled = fixture.ruled(&fixture.disputed(), Some(Verdict::Claimant));
    let mut forged = find(&ruled, &fixture.dispute()).clone();
    forged.owner = addr(&Pubkey::new_unique());

    let mut accounts = ruled.clone();
    replace(&mut accounts, &fixture.dispute(), forged);

    let result = mollusk_at(APPEAL_DEADLINE)
        .process_instruction(&fixture.settle_dispute_ix(fixture.dispute()), &accounts);
    assert!(result.program_result.is_err());
}

/// Unreachable through the escrow's own instructions — kept so that a dispute
/// naming a stranger fails loudly instead of paying a guessed recipient.
#[test]
fn refuses_a_verdict_that_names_a_stranger() {
    let fixture = Fixture::new();
    let ruled = fixture.ruled(&fixture.disputed(), Some(Verdict::Claimant));
    let accounts = with_dispute(&fixture, &ruled, |dispute| {
        dispute.claimant = Pubkey::new_unique();
    });
    let result = mollusk_at(APPEAL_DEADLINE)
        .process_instruction(&fixture.settle_dispute_ix(fixture.dispute()), &accounts);
    assert!(failed_with(&result, DepinError::VerdictNamesAStranger));
}

/// A challenged report is no longer the network's to collect: the plain path
/// refuses a disputed epoch, whatever the clock says.
#[test]
fn keeps_the_plain_payout_closed_while_disputed() {
    let fixture = Fixture::new();
    let result = mollusk_at(NOW + WINDOW)
        .process_instruction(&fixture.settle_report_ix(), &fixture.disputed());
    assert!(failed_with(&result, DepinError::NotReported));
}

/// The error codes are the deployed interface: pin the ones clients match on.
#[test]
fn keeps_the_codes_of_its_errors() {
    assert_eq!(u32::from(DepinError::InvalidParties), 6000);
    assert_eq!(u32::from(DepinError::NotUnderThisDispute), 6015);
    assert_eq!(u32::from(DepinError::Overflow), 6017);
}
