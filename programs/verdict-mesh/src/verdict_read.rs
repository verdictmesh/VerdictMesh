//! Reading a verdict from an integrator's escrow — `FR-012`.
//!
//! The verdict is pulled, not pushed: the escrow reads `Dispute` as plain state and moves its own
//! funds (see docs/PLAN.md). This module is the reading half of that contract, so that every
//! escrow asks the same questions in the same order — and so that when appeals (`FR-021`) and the
//! optimistic track (`FR-024`) change what "the verdict stands" means, integrators update a crate
//! instead of rewriting their settlement.
//!
//! The program itself never calls anything here, and the module exists only under the `cpi`
//! feature integrators depend on this crate with (see `lib.rs`): the deployed program and its IDL
//! do not contain it.
//!
//! **What the escrow still checks itself.** Two things this module cannot know:
//!
//! - That the account is a `Dispute` of this program. Take it as `Account<'info, Dispute>`: Anchor
//!   then checks the owner and the discriminator. An `UncheckedAccount` deserialized by hand lets
//!   anyone lay out a `Dispute` under their own program and write themselves a verdict.
//! - That the dispute is the one recorded in the escrow's **own** state for the funds being
//!   moved, and that the record is cleared on payout. Without it any other dispute over the same
//!   escrow — even one already executed — passes the `escrow_ref` check here, and executing a
//!   verdict twice (`FR-013`) becomes a matter of passing the same account again.

use anchor_lang::prelude::*;

use crate::state::{Dispute, DisputeState, Verdict};

/// Why a verdict cannot be executed yet, or by this escrow at all.
///
/// A range of its own (7000+) rather than variants of `VerdictMeshError`: these errors are
/// returned by the **integrator's** program, whose own errors start at 6000 like every Anchor
/// program's. A shared range would make code 6030 mean one thing in the escrow's IDL and another
/// in the log.
#[error_code(offset = 7000)]
pub enum VerdictReadError {
    #[msg("This dispute was opened over another escrow")]
    NotOurDispute,
    #[msg("The dispute has no verdict yet")]
    VerdictPending,
    #[msg("The verdict is frozen while the dispute is under appeal")]
    VerdictUnderAppeal,
    #[msg("The appeal window has not closed yet")]
    AppealWindowOpen,
}

/// A verdict the escrow may execute now.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Ruling {
    pub verdict: Verdict,
    /// The party whose position won; `None` on `StatusQuo` — "as if there had been no dispute"
    /// (`FR-027a`), so nothing moves.
    ///
    /// `Verdict::Claimant` means "the claimant's position won", not "pay the buyer": the claimant
    /// is whoever opened the dispute, so the direction of payout depends on who that was. The
    /// escrow still has to match the winner against its own parties and refuse a stranger — the
    /// dispute names whoever the escrow put there when opening it.
    pub winner: Option<Pubkey>,
    /// The review deposit the claimant paid, from the dispute's snapshot of the policy rather
    /// than the integrator's current one (`FR-003`): that is the amount `FR-026e` reimburses.
    pub deposit: u64,
}

impl Dispute {
    /// The verdict `escrow` may execute at `now` (unix seconds), or why not.
    ///
    /// The checks, in order: the dispute was opened over `escrow`; it has a verdict; it is not
    /// under appeal; the appeal window has closed. The appeal check is separate from the window
    /// because a dispute under appeal passes both time conditions: it already has a first-round
    /// verdict, and its deadline has already passed.
    ///
    /// `Finalized` is deliberately not required. It is set by `settle_stakes`, which settles the
    /// panel's stakes — VerdictMesh's own business. Waiting for that permissionless crank would
    /// keep the integrator's funds locked until someone cared to send it.
    pub fn ruling(&self, escrow: &Pubkey, now: i64) -> Result<Ruling> {
        require_keys_eq!(self.escrow_ref, *escrow, VerdictReadError::NotOurDispute);
        let verdict = self.verdict.ok_or(VerdictReadError::VerdictPending)?;
        require!(
            self.state != DisputeState::Appealed,
            VerdictReadError::VerdictUnderAppeal
        );
        require!(
            now >= self.appeal_deadline,
            VerdictReadError::AppealWindowOpen
        );

        let winner = match verdict {
            Verdict::Claimant => Some(self.claimant),
            Verdict::Respondent => Some(self.respondent),
            Verdict::StatusQuo => None,
        };

        Ok(Ruling {
            verdict,
            winner,
            deposit: self.policy.deposit,
        })
    }

    /// `ruling` at the cluster's current time — what an escrow's settlement instruction calls.
    pub fn current_ruling(&self, escrow: &Pubkey) -> Result<Ruling> {
        self.ruling(escrow, Clock::get()?.unix_timestamp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Policy;
    use serde_json::Value;

    /// The same table drives the SDK's `verdictStatus`, so the client's prediction and the
    /// on-chain rule are checked against one answer.
    const CASES: &str = include_str!("../../../fixtures/verdict-read/cases.json");

    fn dispute(
        escrow: Pubkey,
        state: DisputeState,
        verdict: Option<Verdict>,
        deadline: i64,
    ) -> Dispute {
        Dispute {
            integrator: Pubkey::new_unique(),
            dispute_id: 0,
            policy: Policy {
                deposit: 5_000_000,
                ..Policy::default()
            },
            escrow_ref: escrow,
            claimant: Pubkey::new_unique(),
            respondent: Pubkey::new_unique(),
            amount: 1,
            state,
            panel: Vec::new(),
            report_hash: [0; 32],
            claimant_claim_hash: [0; 32],
            respondent_claim_hash: [0; 32],
            opened_at: 0,
            entropy_slot: 0,
            commit_deadline: 0,
            reveal_deadline: 0,
            appeal_deadline: deadline,
            votes_claimant: 0,
            votes_respondent: 0,
            escalated: false,
            verdict,
            bump: 0,
        }
    }

    fn state_named(name: &str) -> DisputeState {
        match name {
            "OptimisticPending" => DisputeState::OptimisticPending,
            "Committing" => DisputeState::Committing,
            "Revealing" => DisputeState::Revealing,
            "Tallied" => DisputeState::Tallied,
            "Appealed" => DisputeState::Appealed,
            "Finalized" => DisputeState::Finalized,
            other => panic!("unknown state {other}"),
        }
    }

    fn verdict_named(name: &str) -> Option<Verdict> {
        match name {
            "Claimant" => Some(Verdict::Claimant),
            "Respondent" => Some(Verdict::Respondent),
            "StatusQuo" => Some(Verdict::StatusQuo),
            _ => None,
        }
    }

    fn error_named(name: &str) -> Error {
        match name {
            "NotOurDispute" => VerdictReadError::NotOurDispute.into(),
            "VerdictPending" => VerdictReadError::VerdictPending.into(),
            "VerdictUnderAppeal" => VerdictReadError::VerdictUnderAppeal.into(),
            "AppealWindowOpen" => VerdictReadError::AppealWindowOpen.into(),
            other => panic!("unknown expectation {other}"),
        }
    }

    #[test]
    fn every_case_of_the_shared_table() {
        let table: Value = serde_json::from_str(CASES).unwrap();
        let deadline = table["appealDeadline"].as_i64().unwrap();
        let cases = table["cases"].as_array().unwrap();
        assert!(cases.len() >= 10, "the table lost its cases");

        for case in cases {
            let name = case["name"].as_str().unwrap();
            let escrow = Pubkey::new_unique();
            let opened_over = if case["ours"].as_bool().unwrap() {
                escrow
            } else {
                Pubkey::new_unique()
            };
            let subject = dispute(
                opened_over,
                state_named(case["state"].as_str().unwrap()),
                case["verdict"].as_str().and_then(verdict_named),
                deadline,
            );
            let expect = case["expect"].as_str().unwrap();

            let got = subject.ruling(&escrow, case["now"].as_i64().unwrap());
            match verdict_named(expect) {
                Some(verdict) => assert_eq!(got.unwrap().verdict, verdict, "{name}"),
                None => assert_eq!(got.unwrap_err(), error_named(expect), "{name}"),
            }
        }
    }

    /// The winner follows the role, not a fixed side of the deal.
    #[test]
    fn names_the_party_whose_position_won() {
        let escrow = Pubkey::new_unique();
        let mut subject = dispute(escrow, DisputeState::Tallied, Some(Verdict::Claimant), 0);
        assert_eq!(
            subject.ruling(&escrow, 0).unwrap().winner,
            Some(subject.claimant)
        );

        subject.verdict = Some(Verdict::Respondent);
        assert_eq!(
            subject.ruling(&escrow, 0).unwrap().winner,
            Some(subject.respondent)
        );

        subject.verdict = Some(Verdict::StatusQuo);
        assert_eq!(subject.ruling(&escrow, 0).unwrap().winner, None);
    }

    #[test]
    fn reimburses_the_deposit_of_the_snapshot() {
        let escrow = Pubkey::new_unique();
        let subject = dispute(escrow, DisputeState::Tallied, Some(Verdict::Claimant), 0);
        assert_eq!(subject.ruling(&escrow, 0).unwrap().deposit, 5_000_000);
    }

    /// The integrator's own errors start at 6000; these must not land among them.
    #[test]
    fn errors_live_outside_the_integrators_range() {
        assert_eq!(u32::from(VerdictReadError::NotOurDispute), 7000);
        assert_eq!(u32::from(VerdictReadError::AppealWindowOpen), 7003);
    }
}
