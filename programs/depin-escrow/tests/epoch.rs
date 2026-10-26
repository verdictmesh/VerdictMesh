//! Funding, reporting and the unchallenged payout — the epoch without
//! VerdictMesh in it.

#[allow(dead_code)]
#[path = "harness.rs"]
mod harness;

use anchor_lang::solana_program::pubkey::Pubkey;
use depin_escrow::{
    events::{EpochFunded, EpochSettled, UsageReported},
    DepinError, EpochState,
};
use harness::*;

// ── funding ─────────────────────────────────────────────────────────────────

#[test]
fn locks_the_whole_reward_in_the_epoch_vault() {
    let fixture = Fixture::new();
    let accounts = fixture.funded();

    assert_eq!(fixture.balance(&accounts, &fixture.vault), reward());
    assert_eq!(
        fixture.balance(&accounts, &fixture.network_tokens),
        usdc(500) - reward()
    );
    assert_eq!(
        token_state(find(&accounts, &fixture.vault)).owner,
        addr(&fixture.epoch)
    );
}

#[test]
fn writes_the_parties_the_policy_and_the_window() {
    let fixture = Fixture::new();
    let epoch = fixture.epoch_state(&fixture.funded());

    assert_eq!(epoch.network, fixture.network);
    assert_eq!(epoch.operator, fixture.operator);
    assert_eq!(epoch.mint, fixture.mint);
    assert_eq!(epoch.integrator, fixture.integrator);
    assert_eq!(epoch.reward, reward());
    assert_eq!(epoch.challenge_window, WINDOW);
    assert_eq!(epoch.epoch_id, EPOCH_ID);
    assert_eq!(epoch.state, EpochState::Funded);
}

/// With the epoch the operator accepts the policy they will be judged by.
#[test]
fn requires_the_operator_to_sign() {
    let fixture = Fixture::new();
    let mut ix = fixture.fund_ix(reward(), WINDOW);
    for meta in &mut ix.accounts {
        if meta.pubkey == addr(&fixture.operator) {
            meta.is_signer = false;
        }
    }

    let result = mollusk().process_instruction(&ix, &fixture.accounts);
    assert!(result.program_result.is_err());
}

#[test]
fn refuses_a_zero_reward() {
    let fixture = Fixture::new();
    let result = mollusk().process_instruction(&fixture.fund_ix(0, WINDOW), &fixture.accounts);
    assert!(failed_with(&result, DepinError::InvalidReward));
}

#[test]
fn refuses_a_window_that_is_not_positive() {
    let fixture = Fixture::new();
    for window in [0, -1] {
        let result =
            mollusk().process_instruction(&fixture.fund_ix(reward(), window), &fixture.accounts);
        assert!(failed_with(&result, DepinError::InvalidWindow));
    }
}

#[test]
fn refuses_a_policy_that_points_at_another_escrow_program() {
    let fixture = Fixture::new();
    let stranger = Pubkey::new_unique();
    let (integrator, _) = mesh_integrator_pda(&stranger);

    let mut accounts = fixture.accounts.clone();
    accounts.push((
        addr(&integrator),
        mesh_integrator_account(&stranger, &Pubkey::new_unique()),
    ));

    let ix = fixture.fund_ix_with(integrator, fixture.mint, reward(), WINDOW);
    let result = mollusk().process_instruction(&ix, &accounts);
    assert!(failed_with(&result, DepinError::WrongArbitrationProgram));
}

/// The deposit is reimbursed out of the loser's share, which only works when
/// the reward is in the asset the deposit was paid in.
#[test]
fn refuses_a_reward_outside_the_settlement_asset() {
    let fixture = Fixture::new();
    let other = Pubkey::new_unique();

    let mut accounts = fixture.accounts.clone();
    accounts.push((addr(&other), spl_mint(DECIMALS as u8)));
    replace(
        &mut accounts,
        &fixture.network_tokens,
        token_account(&other, &fixture.network, usdc(500)),
    );

    let ix = fixture.fund_ix_with(fixture.integrator, other, reward(), WINDOW);
    let result = mollusk().process_instruction(&ix, &accounts);
    assert!(failed_with(&result, DepinError::WrongRewardMint));
}

#[test]
fn emits_the_funding_event() {
    let fixture = Fixture::new();
    let (mollusk, logs) = mollusk_with_logs();
    let result = mollusk.process_instruction(&fixture.fund_ix(reward(), WINDOW), &fixture.accounts);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let events = emitted::<EpochFunded>(&logs);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].epoch, fixture.epoch);
    assert_eq!(events[0].reward, reward());
    assert_eq!(events[0].challenge_window, WINDOW);
}

// ── reporting ───────────────────────────────────────────────────────────────

#[test]
fn opens_the_challenge_window_at_the_report() {
    let fixture = Fixture::new();
    let accounts = fixture.after(
        &fixture.funded(),
        NOW + 50,
        &fixture.report_ix(REPORTED_BPS),
    );

    assert_eq!(
        fixture.epoch_state(&accounts).state,
        EpochState::Reported {
            operator_bps: REPORTED_BPS,
            challenge_deadline: NOW + 50 + WINDOW,
        }
    );
}

#[test]
fn takes_the_report_from_the_network_only() {
    let fixture = Fixture::new();
    let mut accounts = fixture.funded();
    accounts.push((addr(&fixture.operator), wallet(1)));

    let result =
        mollusk().process_instruction(&fixture.report_ix_by(fixture.operator, 10_000), &accounts);
    assert!(failed_with(&result, DepinError::NotTheNetwork));
}

#[test]
fn takes_one_report_per_epoch() {
    let fixture = Fixture::new();
    let result = mollusk().process_instruction(&fixture.report_ix(1_000), &fixture.reported());
    assert!(failed_with(&result, DepinError::AlreadyReported));
}

#[test]
fn refuses_a_share_above_the_whole() {
    let fixture = Fixture::new();
    let result = mollusk().process_instruction(&fixture.report_ix(10_001), &fixture.funded());
    assert!(failed_with(&result, DepinError::InvalidShare));
}

#[test]
fn emits_the_report_event() {
    let fixture = Fixture::new();
    let (mollusk, logs) = mollusk_with_logs();
    let result = mollusk.process_instruction(&fixture.report_ix(REPORTED_BPS), &fixture.funded());
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let events = emitted::<UsageReported>(&logs);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].operator_bps, REPORTED_BPS);
    assert_eq!(events[0].challenge_deadline, NOW + WINDOW);
}

// ── the unchallenged payout ─────────────────────────────────────────────────

#[test]
fn pays_both_parties_their_share_once_the_window_closes() {
    let fixture = Fixture::new();
    let accounts = fixture.after(
        &fixture.reported(),
        NOW + WINDOW,
        &fixture.settle_report_ix(),
    );

    assert_eq!(
        fixture.balance(&accounts, &fixture.operator_tokens),
        usdc(50) + usdc(60)
    );
    assert_eq!(
        fixture.balance(&accounts, &fixture.network_tokens),
        usdc(500) - reward() + usdc(40)
    );
    assert_eq!(fixture.balance(&accounts, &fixture.vault), 0);
    assert_eq!(
        fixture.epoch_state(&accounts).state,
        EpochState::Settled {
            operator_bps: REPORTED_BPS
        }
    );
}

#[test]
fn holds_the_payout_while_the_window_is_open() {
    let fixture = Fixture::new();
    let result = mollusk_at(NOW + WINDOW - 1)
        .process_instruction(&fixture.settle_report_ix(), &fixture.reported());
    assert!(failed_with(&result, DepinError::ChallengeWindowOpen));
}

#[test]
fn pays_nothing_before_a_report() {
    let fixture = Fixture::new();
    let result = mollusk_at(NOW + WINDOW)
        .process_instruction(&fixture.settle_report_ix(), &fixture.funded());
    assert!(failed_with(&result, DepinError::NotReported));
}

/// `FR-013` for the plain path: the second call finds a settled epoch.
#[test]
fn pays_a_report_once() {
    let fixture = Fixture::new();
    let settled = fixture.after(
        &fixture.reported(),
        NOW + WINDOW,
        &fixture.settle_report_ix(),
    );
    let result =
        mollusk_at(NOW + WINDOW).process_instruction(&fixture.settle_report_ix(), &settled);
    assert!(failed_with(&result, DepinError::NotReported));
}

/// Permissionless does not mean steerable: the payout goes to the parties'
/// accounts or nowhere.
#[test]
fn refuses_to_pay_a_share_into_somebody_else_s_account() {
    let fixture = Fixture::new();
    let thief = Pubkey::new_unique();
    let mut accounts = fixture.reported();
    replace(
        &mut accounts,
        &fixture.operator_tokens,
        token_account(&fixture.mint, &thief, 0),
    );

    let result =
        mollusk_at(NOW + WINDOW).process_instruction(&fixture.settle_report_ix(), &accounts);
    assert!(result.program_result.is_err());
}

/// A report of zero leaves the operator's account untouched — no zero-amount
/// transfer, and nothing to pay.
#[test]
fn pays_the_whole_reward_back_on_a_report_of_zero() {
    let fixture = Fixture::new();
    let reported = fixture.after(&fixture.funded(), NOW, &fixture.report_ix(0));
    let accounts = fixture.after(&reported, NOW + WINDOW, &fixture.settle_report_ix());

    assert_eq!(
        fixture.balance(&accounts, &fixture.operator_tokens),
        usdc(50)
    );
    assert_eq!(
        fixture.balance(&accounts, &fixture.network_tokens),
        usdc(500)
    );
}

#[test]
fn emits_the_settlement_event_without_a_dispute() {
    let fixture = Fixture::new();
    let (mut mollusk, logs) = mollusk_with_logs();
    mollusk.sysvars.clock.unix_timestamp = NOW + WINDOW;
    let result = mollusk.process_instruction(&fixture.settle_report_ix(), &fixture.reported());
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let events = emitted::<EpochSettled>(&logs);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].dispute, None);
    assert_eq!(events[0].operator_amount, usdc(60));
    assert_eq!(events[0].network_amount, usdc(40));
    assert_eq!(events[0].reimbursed, 0);
}
