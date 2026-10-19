use anchor_lang::prelude::*;

use crate::{
    errors::VerdictMeshError,
    events::VoteCommitted,
    seeds,
    state::{Dispute, DisputeState, VoteCommit},
};

/// The first voting step — `FR-008`: the juror records a commitment to their vote
/// without revealing the vote itself.
///
/// **What commit-reveal does and does not do.** It upholds `FR-009`: until the
/// commit window closes, state contains nothing from which another juror's choice
/// can be derived — only a hash. There is no way to vote "with the majority" after
/// looking at votes already cast. But a commitment on its own binds to nothing:
/// that the juror reveals exactly this one is enforced by the check on reveal
/// (`FR-008a`, T018), and that they reveal at all — by slashing for silence
/// (`FR-008b`, T020). This instruction only puts a value into an account, and all
/// three pieces must fit together, or the mechanism does not work.
///
/// **The commitment formula is neither computed nor checked here** — it lives in
/// `crate::vote`. Verifying the hash without knowing the secret is impossible by
/// construction, so the only thing the program can do at this point is store the
/// submitted value and bind it to the "dispute + juror" pair via the account address.
///
/// **A zero commitment is not rejected, and that is a decision.** In `open_dispute`
/// a zero claim hash meant "field absent", because the field lives in a zeroed
/// account. Here "no commitment" is signalled by the absence of the account itself,
/// and a commitment with no preimage is no better and no worse than `hash(garbage)`:
/// neither will ever be revealed, and both cost the juror part of the stake under
/// `FR-008b`. A zero check would give the impression of protecting against something
/// it does not protect against.
impl CommitVote<'_> {
    pub fn handle(ctx: Context<CommitVote>, commitment: [u8; 32]) -> Result<()> {
        let juror = ctx.accounts.juror.key();
        let dispute = &ctx.accounts.dispute;

        require!(
            dispute.state == DisputeState::Committing,
            VerdictMeshError::WrongState
        );
        // Strictly less: the deadline itself is already outside the window.
        // Otherwise the commit window overlaps the reveal window by one second,
        // and that second is enough to commit after seeing someone else's vote.
        require!(
            Clock::get()?.unix_timestamp < dispute.commit_deadline,
            VerdictMeshError::WindowClosed
        );
        // The panel votes, not volunteers. Without this check an outsider's
        // commitment fails on nothing else — its account is derived from their own
        // wallet and is created just fine — and it would only be counted in `tally`.
        require!(dispute.panel.contains(&juror), VerdictMeshError::NotOnPanel);

        let vote = &mut ctx.accounts.vote;
        vote.dispute = dispute.key();
        vote.juror = juror;
        vote.commitment = commitment;
        // No vote here until reveal — `FR-009`. An empty `choice` later is precisely
        // the sign that the juror stayed silent (`FR-008b`, T020).
        vote.choice = None;
        // A commitment belongs to the round it was submitted in — see `Dispute::round`.
        vote.round = dispute.round();
        vote.bump = ctx.bumps.vote;

        // The event carries neither the commitment nor any hint of the choice:
        // `FR-029` requires a reconstructable timeline, not readable votes. The
        // watcher (T027) builds the list of jurors who have already committed from it.
        emit!(VoteCommitted {
            dispute: dispute.key(),
            juror,
        });

        Ok(())
    }
}

#[derive(Accounts)]
pub struct CommitVote<'info> {
    /// The juror pays rent for their own vote account. The signature is the only
    /// proof of authorship of the commitment: the signer's wallet goes into both the
    /// account address and the hash itself (`crate::vote`).
    #[account(mut)]
    pub juror: Signer<'info>,

    /// Read-only. `Dispute` deliberately has no counter of submitted commitments:
    /// quorum is counted over **revealed** votes (`FR-010`), and who committed but
    /// stayed silent is visible from the vote accounts, which have to be iterated
    /// for slashing anyway.
    #[account(
        seeds = [
            seeds::DISPUTE,
            dispute.integrator.as_ref(),
            &dispute.dispute_id.to_le_bytes(),
        ],
        bump = dispute.bump,
    )]
    pub dispute: Account<'info, Dispute>,

    /// `init`, not `init_if_needed`: a commitment is submitted once. A second
    /// submission is not "changing one's mind" but a chance to wait for someone
    /// else's reveal and rewrite one's own vote to match it.
    ///
    /// The address is derived from the "dispute + juror" pair, so a commitment cannot
    /// be written into someone else's account, and each juror has exactly one per dispute.
    #[account(
        init,
        payer = juror,
        space = VoteCommit::DISCRIMINATOR.len() + VoteCommit::INIT_SPACE,
        seeds = [seeds::VOTE, dispute.key().as_ref(), juror.key().as_ref()],
        bump,
    )]
    pub vote: Account<'info, VoteCommit>,

    pub system_program: Program<'info, System>,
}
