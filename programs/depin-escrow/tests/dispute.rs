//! Challenging a report — the one CPI into VerdictMesh's `open_dispute`, run
//! against the real VerdictMesh bytecode.

#[allow(dead_code)]
#[path = "harness.rs"]
mod harness;

use anchor_lang::solana_program::pubkey::Pubkey;
use depin_escrow::{claims::claim_of, events::EpochDisputed, DepinError, EpochState};
use harness::*;
use verdict_mesh::state::Dispute;

/// The main test of the file: the **escrow** opens the dispute, signing with
/// its own PDA, and VerdictMesh accepts the signature.
#[test]
fn opens_a_dispute_over_the_contested_part_through_verdict_mesh() {
    let fixture = Fixture::new();
    let accounts = fixture.disputed();

    let dispute: Dispute = decode(find(&accounts, &fixture.dispute()));
    assert_eq!(dispute.escrow_ref, fixture.epoch);
    assert_eq!(dispute.claimant, fixture.operator);
    assert_eq!(dispute.respondent, fixture.network);
    // 90 % against 60 % of 100: the 60 are paid whichever way it goes, the
    // 30 between the offers are what the panel decides.
    assert_eq!(dispute.amount, usdc(30));
}

#[test]
fn records_both_offers_and_the_dispute_in_the_epoch() {
    let fixture = Fixture::new();
    let accounts = fixture.disputed();

    assert_eq!(
        fixture.epoch_state(&accounts).state,
        EpochState::Disputed {
            dispute: fixture.dispute(),
            reported_bps: REPORTED_BPS,
            claimed_bps: CLAIMED_BPS,
        }
    );
}

/// The positions are the offers, and the program fingerprints both from what
/// is in the epoch — the operator never words the network's position.
#[test]
fn fingerprints_each_side_by_its_offer() {
    let fixture = Fixture::new();
    let dispute: Dispute = decode(find(&fixture.disputed(), &fixture.dispute()));

    assert_eq!(
        dispute.claimant_claim_hash,
        claim_of(&fixture.epoch, CLAIMED_BPS)
    );
    assert_eq!(
        dispute.respondent_claim_hash,
        claim_of(&fixture.epoch, REPORTED_BPS)
    );
}

/// The operator pays the deposit (`FR-026`); the reward stays where it is —
/// the dispute gives VerdictMesh no hold over it.
#[test]
fn charges_the_operator_the_deposit_and_leaves_the_reward_alone() {
    let fixture = Fixture::new();
    let accounts = fixture.disputed();
    let deposit = demo_policy().deposit;

    assert_eq!(
        fixture.balance(&accounts, &fixture.operator_tokens),
        usdc(50) - deposit
    );
    assert_eq!(
        fixture.balance(&accounts, &mesh_dispute_vault_pda(&fixture.dispute()).0),
        deposit
    );
    assert_eq!(fixture.balance(&accounts, &fixture.vault), reward());
}

#[test]
fn refuses_a_challenge_from_anyone_but_the_operator() {
    let fixture = Fixture::new();
    let ix = fixture.dispute_ix_with(fixture.network, fixture.integrator, CLAIMED_BPS);
    let result = mollusk_at(NOW + 1).process_instruction(&ix, &fixture.reported());
    assert!(failed_with(&result, DepinError::NotTheOperator));
}

#[test]
fn refuses_a_challenge_once_the_window_has_closed() {
    let fixture = Fixture::new();
    let result = mollusk_at(NOW + WINDOW)
        .process_instruction(&fixture.dispute_ix(CLAIMED_BPS), &fixture.reported());
    assert!(failed_with(&result, DepinError::ChallengeWindowClosed));
}

#[test]
fn accepts_a_challenge_in_the_last_second_of_the_window() {
    let fixture = Fixture::new();
    let result = mollusk_at(NOW + WINDOW - 1)
        .process_instruction(&fixture.dispute_ix(CLAIMED_BPS), &fixture.reported());
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);
}

#[test]
fn refuses_a_claim_that_is_not_above_the_report() {
    let fixture = Fixture::new();
    for claimed in [REPORTED_BPS, REPORTED_BPS - 1, 0] {
        let result = mollusk_at(NOW + 1)
            .process_instruction(&fixture.dispute_ix(claimed), &fixture.reported());
        assert!(failed_with(&result, DepinError::ClaimNotAboveReport));
    }
}

#[test]
fn refuses_a_claim_above_the_whole() {
    let fixture = Fixture::new();
    let result =
        mollusk_at(NOW + 1).process_instruction(&fixture.dispute_ix(10_001), &fixture.reported());
    assert!(failed_with(&result, DepinError::InvalidShare));
}

/// Two offers can differ in basis points and still round to the same tokens.
/// That is the escrow's refusal, not VerdictMesh's `InvalidAmount`.
#[test]
fn refuses_a_challenge_that_contests_nothing() {
    let fixture = Fixture::new();
    let funded = fixture.after(&fixture.accounts, NOW, &fixture.fund_ix(1, WINDOW));
    let reported = fixture.after(&funded, NOW, &fixture.report_ix(REPORTED_BPS));

    let result =
        mollusk_at(NOW + 1).process_instruction(&fixture.dispute_ix(REPORTED_BPS + 1), &reported);
    assert!(failed_with(&result, DepinError::NothingContested));
}

#[test]
fn refuses_a_challenge_before_the_report() {
    let fixture = Fixture::new();
    let result = mollusk_at(NOW + 1)
        .process_instruction(&fixture.dispute_ix(CLAIMED_BPS), &fixture.funded());
    assert!(failed_with(&result, DepinError::NotReported));
}

#[test]
fn refuses_a_second_challenge() {
    let fixture = Fixture::new();
    let result =
        mollusk_at(NOW + 2).process_instruction(&fixture.dispute_ix(10_000), &fixture.disputed());
    assert!(failed_with(&result, DepinError::NotReported));
}

#[test]
fn refuses_a_dispute_under_a_policy_the_epoch_never_agreed_to() {
    let fixture = Fixture::new();
    let stranger = Pubkey::new_unique();
    let (other, _) = mesh_integrator_pda(&stranger);

    let mut accounts = fixture.reported();
    accounts.push((
        addr(&other),
        mesh_integrator_account(&stranger, &ESCROW_PROGRAM),
    ));

    let ix = fixture.dispute_ix_with(fixture.operator, other, CLAIMED_BPS);
    let result = mollusk_at(NOW + 1).process_instruction(&ix, &accounts);
    assert!(failed_with(&result, DepinError::WrongIntegrator));
}

/// VerdictMesh judges funds it never sees: neither the reward vault nor the
/// network's account is among the instruction's accounts.
#[test]
fn never_shows_the_reward_vault_to_the_arbitration_program() {
    let fixture = Fixture::new();
    let metas = fixture.dispute_ix(CLAIMED_BPS).accounts;

    for hidden in [&fixture.vault, &fixture.network_tokens] {
        assert!(
            !metas.iter().any(|meta| meta.pubkey == addr(hidden)),
            "the arbitration call must not carry the funds it judges"
        );
    }
}

/// The escrow does not repeat VerdictMesh's constraints: a forged `config` is
/// refused by **VerdictMesh**.
#[test]
fn leaves_the_arbitration_accounts_to_the_arbitration_program() {
    let fixture = Fixture::new();
    let impostor = Pubkey::new_unique();

    let mut accounts = fixture.reported();
    accounts.push((addr(&impostor), missing()));

    let mut ix = fixture.dispute_ix(CLAIMED_BPS);
    for meta in &mut ix.accounts {
        if meta.pubkey == addr(&mesh_config_pda().0) {
            meta.pubkey = addr(&impostor);
        }
    }

    let result = mollusk_at(NOW + 1).process_instruction(&ix, &accounts);
    assert!(result.program_result.is_err());
    assert!(!failed_with(&result, DepinError::WrongIntegrator));
}

#[test]
fn emits_the_dispute_event() {
    let fixture = Fixture::new();
    let (mut mollusk, logs) = mollusk_with_logs();
    mollusk.sysvars.clock.unix_timestamp = NOW + 1;
    let result = mollusk.process_instruction(&fixture.dispute_ix(CLAIMED_BPS), &fixture.reported());
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let events = emitted::<EpochDisputed>(&logs);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].dispute, fixture.dispute());
    assert_eq!(events[0].reported_bps, REPORTED_BPS);
    assert_eq!(events[0].claimed_bps, CLAIMED_BPS);
    assert_eq!(events[0].contested, usdc(30));
}
