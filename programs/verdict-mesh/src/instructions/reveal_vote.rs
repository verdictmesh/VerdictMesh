use anchor_lang::prelude::*;

use crate::{
    errors::VerdictMeshError,
    events::VoteRevealed,
    seeds,
    state::{Ballot, Dispute, DisputeState, VoteCommit},
    vote::{commitment_of, SALT_LEN},
};

/// The second voting step — `FR-008a`: the juror discloses the vote and the secret
/// against which the previously submitted commitment is checked.
///
/// **This is the instruction that gives the first step its meaning.** A commitment on
/// its own binds to nothing — it is just a number. What makes it binding is that it
/// can be revealed by exactly one "choice + secret" pair, and the vote counts only
/// when that pair matches.
///
/// **A mismatch rejects the transaction rather than "not counting the vote".** The
/// wording of `FR-008a` ("not counted in the tally") would also allow silently
/// swallowing it, but then a juror who got the secret wrong would receive a
/// confirmed transaction and learn about losing the stake only at settlement.
/// Rejection leaves them time within the reveal window.
///
/// **Nothing but the hash needs checking here, and that is a property of the address.**
/// The vote account is derived from the "dispute + juror" pair, so its existence already
/// proves the signer was on the panel when committing (`commit_vote` does not create the
/// account otherwise). Re-checking the panel would add nothing.
impl RevealVote<'_> {
    pub fn handle(ctx: Context<RevealVote>, choice: Ballot, salt: [u8; SALT_LEN]) -> Result<()> {
        let dispute_key = ctx.accounts.dispute.key();
        let juror = ctx.accounts.juror.key();
        let now = Clock::get()?.unix_timestamp;

        let dispute = &mut ctx.accounts.dispute;
        require!(
            matches!(
                dispute.state,
                DisputeState::Committing | DisputeState::Revealing
            ),
            VerdictMeshError::WrongState
        );
        // `FR-009` at the boundary: revealing before the commit window ends means
        // showing one's vote to those who have not committed yet. The windows are
        // adjacent — the commit deadline itself already belongs to the reveal.
        require!(now >= dispute.commit_deadline, VerdictMeshError::WindowOpen);
        require!(
            now < dispute.reveal_deadline,
            VerdictMeshError::WindowClosed
        );

        let vote = &mut ctx.accounts.vote;
        // Once only: a second reveal would count the same vote twice, and a panel
        // of three would reach quorum on the strength of a single juror.
        require!(vote.choice.is_none(), VerdictMeshError::AlreadyRevealed);
        // A first-round commitment not revealed before the tally is late for good.
        // Allowing it now would let the juror decide whether to publish their vote
        // **after seeing the first tally** — and thereby cancel the slashing that
        // `FR-027b` imposes precisely for that silence.
        require!(
            vote.round == dispute.round(),
            VerdictMeshError::StaleCommitment
        );
        require!(
            commitment_of(&dispute_key, &juror, choice, &salt) == vote.commitment,
            VerdictMeshError::CommitmentMismatch
        );

        vote.choice = Some(choice);

        // The counters live in the dispute rather than being derived by iterating
        // vote accounts: otherwise the tally (T019) would have to receive the whole
        // panel as accounts and would hit the transaction limit where two numbers
        // suffice. The accounts will be iterated anyway — but in stake settlement
        // (T020), which needs them individually.
        let counter = match choice {
            Ballot::Claimant => &mut dispute.votes_claimant,
            Ballot::Respondent => &mut dispute.votes_respondent,
        };
        *counter = counter.checked_add(1).ok_or(VerdictMeshError::Overflow)?;

        // The first reveal closes commits in state too, not only by the clock.
        dispute.state = DisputeState::Revealing;

        emit!(VoteRevealed {
            dispute: dispute_key,
            juror,
            choice,
        });

        Ok(())
    }
}

#[derive(Accounts)]
pub struct RevealVote<'info> {
    /// The juror's signature is what ties the vote account to whoever submitted it:
    /// the signer's wallet goes into both the account address and the hash itself.
    /// So no one can reveal another juror's vote with their own signature — neither
    /// on behalf of a juror who chose to stay silent, nor against them.
    ///
    /// `mut` is not needed: revealing creates no accounts and refunds no rent.
    pub juror: Signer<'info>,

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

    /// The account is not closed after reveal: slashing (`FR-011`, `FR-008b`,
    /// T020) reads exactly this account to tell a correct vote from a losing one,
    /// and both from silence. The rent is returned to the juror there.
    #[account(
        mut,
        seeds = [seeds::VOTE, dispute.key().as_ref(), juror.key().as_ref()],
        bump = vote.bump,
    )]
    pub vote: Account<'info, VoteCommit>,
}
