use anchor_lang::prelude::*;

use crate::{
    errors::VerdictMeshError,
    events::{DisputeEscalated, DisputeTallied},
    seeds,
    state::{Dispute, DisputeState, Verdict},
};

/// Vote tally — `FR-010`, `FR-027`, `FR-027a`.
///
/// **A dispute must end, and this instruction is what ends it.** `SC-011` requires an outcome
/// in 100% of reviews where part of the panel stayed silent. So there are exactly three exits,
/// all of them final: quorum and a majority — a verdict; otherwise — one extended panel; if
/// that fails too — status quo. There is no third round.
///
/// **Quorum and majority are different questions.** Quorum answers whether
/// enough jurors spoke; majority, whether that yields an answer. A tie with
/// quorum reached produces no verdict and escalates just as silence does.
///
/// **Escalation does not reset first-round votes.** The extended panel is the
/// same review, continued with more jurors. Resetting the counters would make
/// already revealed `VoteCommit`s unrevealable forever (`AlreadyRevealed` is
/// never cleared), and would force those who voted honestly to vote a second
/// time for nothing.
///
/// **The instruction is permissionless.** A crank with no signature required: the transition
/// depends on time, not on anyone's will, and restricting it to a single party would only give
/// that party the ability to never call the tally at all.
///
/// **`active_disputes` is not decremented here, and that is not an oversight.** A dispute with
/// a verdict is not yet finalized — the appeal window and stake settlement are still ahead. A
/// juror released here could leave the registry with their stake (`FR-007`, T015) moments
/// before they were due to be slashed. The counter is cleared by T020, which walks the whole
/// panel juror by juror anyway.
impl Tally<'_> {
    pub fn handle(ctx: Context<Tally>) -> Result<()> {
        let clock = Clock::get()?;
        let now = clock.unix_timestamp;
        let dispute_key = ctx.accounts.dispute.key();
        let dispute = &mut ctx.accounts.dispute;

        require!(
            matches!(
                dispute.state,
                DisputeState::Committing | DisputeState::Revealing
            ),
            VerdictMeshError::WrongState
        );
        // Tallying before the reveal window ends means tallying without those
        // who could still reveal. There is no upper bound: a dispute that gets
        // attention late must still be tallied, not get stuck.
        require!(now >= dispute.reveal_deadline, VerdictMeshError::WindowOpen);

        let policy = dispute.policy;
        let quorum = if dispute.escalated {
            policy.extended_quorum
        } else {
            policy.quorum
        };
        let revealed = dispute
            .votes_claimant
            .checked_add(dispute.votes_respondent)
            .ok_or(VerdictMeshError::Overflow)?;

        let verdict = match dispute.votes_claimant.cmp(&dispute.votes_respondent) {
            _ if revealed < quorum => None,
            std::cmp::Ordering::Greater => Some(Verdict::Claimant),
            std::cmp::Ordering::Less => Some(Verdict::Respondent),
            std::cmp::Ordering::Equal => None,
        };

        match verdict {
            Some(verdict) => Self::decide(dispute, dispute_key, verdict, now, policy.appeal_window),
            None if dispute.escalated => {
                // `FR-027a`: the extended panel produced no outcome either. Funds
                // are distributed as if there had been no dispute.
                Self::decide(
                    dispute,
                    dispute_key,
                    Verdict::StatusQuo,
                    now,
                    policy.appeal_window,
                )
            }
            None => Self::escalate(dispute, dispute_key, now, clock.slot),
        }
    }

    fn decide(
        dispute: &mut Dispute,
        dispute_key: Pubkey,
        verdict: Verdict,
        now: i64,
        appeal_window: i64,
    ) -> Result<()> {
        let appeal_deadline = now
            .checked_add(appeal_window)
            .ok_or(VerdictMeshError::Overflow)?;

        dispute.verdict = Some(verdict);
        dispute.state = DisputeState::Tallied;
        // The appeal window counts from the verdict, not from dispute opening:
        // until this moment its length meant nothing.
        dispute.appeal_deadline = appeal_deadline;

        emit!(DisputeTallied {
            dispute: dispute_key,
            verdict,
            votes_claimant: dispute.votes_claimant,
            votes_respondent: dispute.votes_respondent,
            appeal_deadline,
        });

        Ok(())
    }

    /// Move to the extended panel — `FR-027`. The panel itself is topped up by
    /// `select_panel`: pulling the registry through the tally would make the
    /// permissionless crank exactly as expensive as the registry is large.
    fn escalate(dispute: &mut Dispute, dispute_key: Pubkey, now: i64, slot: u64) -> Result<()> {
        let commit_deadline = now
            .checked_add(dispute.policy.commit_window)
            .ok_or(VerdictMeshError::Overflow)?;
        let reveal_deadline = commit_deadline
            .checked_add(dispute.policy.reveal_window)
            .ok_or(VerdictMeshError::Overflow)?;

        dispute.escalated = true;
        dispute.state = DisputeState::Committing;
        dispute.commit_deadline = commit_deadline;
        dispute.reveal_deadline = reveal_deadline;
        // **Entropy is re-pinned, and this is the second known weak spot of
        // its source.** `SlotHashes` remembers 512 slots — about 3.5 minutes —
        // while both windows pass between opening the dispute and escalation,
        // i.e. as long as the integrator set in the policy. By this point the
        // opening slot no longer exists, and there would be nothing to draw
        // the extended panel's jurors from: escalation would lead to a dead
        // end, and `SC-011` would not be met at all.
        //
        // The cost, stated openly: whoever calls the tally chooses the slot to do it
        // in, and so can re-roll the top-up by retrying. The only check on this is that
        // the instruction is permissionless — the first call pins the slot, and a
        // griefer must beat everyone who cares about the dispute, exactly once. Moving
        // to VRF (`FR-006a`) removes both this and the slot leader's influence in one
        // move, touching nothing outside `panel.rs`.
        dispute.entropy_slot = slot.saturating_sub(1);

        emit!(DisputeEscalated {
            dispute: dispute_key,
            votes_claimant: dispute.votes_claimant,
            votes_respondent: dispute.votes_respondent,
            commit_deadline,
            reveal_deadline,
        });

        Ok(())
    }
}

#[derive(Accounts)]
pub struct Tally<'info> {
    #[account(
        mut,
        seeds = [
            seeds::DISPUTE,
            dispute.integrator.as_ref(),
            &dispute.dispute_id.to_le_bytes(),
        ],
        bump = dispute.bump,
    )]
    pub dispute: Account<'info, Dispute>,
}
