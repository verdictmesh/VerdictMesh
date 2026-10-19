use anchor_lang::prelude::*;

pub mod errors;
pub mod events;
pub mod instructions;
pub mod panel;
pub mod seeds;
pub mod state;
pub mod vault;
pub mod vote;

pub use errors::VerdictMeshError;
pub use instructions::*;
pub use state::*;
pub use vote::SALT_LEN;

declare_id!("8WyWpDD1ZbkTRGG6SRcYyWxApPsHaSgWn2SWJQ8xSgxq");

/// VerdictMesh — a protocol-agnostic dispute resolution layer.
///
/// The program never holds integrator funds: the escrow reads the `Dispute` account as
/// plain state and distributes the funds itself (see docs/PLAN.md → "the verdict is
/// pulled, not pushed"). That is why there is no instruction here capable of moving
/// someone else's money — FR-014 is a property of the design, not a promise.
#[program]
pub mod verdict_mesh {
    use super::*;

    /// One-time protocol initialization: the settlement asset, the reporter role key
    /// and the treasury address. The program has no instruction that changes them
    /// afterwards — see `instructions::initialize`.
    pub fn initialize(ctx: Context<Initialize>, reporter: Pubkey, treasury: Pubkey) -> Result<()> {
        Initialize::handle(ctx, reporter, treasury)
    }

    /// Binds an arbitration policy to the integrator. The policy is validated
    /// once, here, and is copied into every dispute as a snapshot — see
    /// `instructions::register_integrator`.
    pub fn register_integrator(ctx: Context<RegisterIntegrator>, policy: Policy) -> Result<()> {
        RegisterIntegrator::handle(ctx, policy)
    }

    /// Opens a dispute over a locked balance. Called by the escrow program
    /// with its own signature — see `instructions::open_dispute`.
    pub fn open_dispute(
        ctx: Context<OpenDispute>,
        claimant: Pubkey,
        respondent: Pubkey,
        amount: u64,
        claimant_claim_hash: [u8; 32],
        respondent_claim_hash: [u8; 32],
    ) -> Result<()> {
        OpenDispute::handle(
            ctx,
            claimant,
            respondent,
            amount,
            claimant_claim_hash,
            respondent_claim_hash,
        )
    }

    /// Selects the juror panel for an open dispute. Permissionless: the outcome
    /// is deterministic and was already fixed when the dispute was opened — see
    /// `instructions::select_panel`.
    pub fn select_panel<'info>(
        ctx: Context<'_, '_, 'info, 'info, SelectPanel<'info>>,
    ) -> Result<()> {
        SelectPanel::handle(ctx)
    }

    /// Deposits a stake and adds the juror to the registry. There is no entry
    /// threshold: whether the stake is sufficient is decided by the policy of the
    /// dispute whose panel the juror lands on — see `instructions::stake`.
    pub fn stake(ctx: Context<Stake>, amount: u64) -> Result<()> {
        Stake::handle(ctx, amount)
    }

    /// Returns the stake and removes the juror from the registry via swap-remove.
    /// While the juror holds an unfinalized dispute, there is no way out — see
    /// `instructions::unstake`.
    pub fn unstake(ctx: Context<Unstake>) -> Result<()> {
        Unstake::handle(ctx)
    }

    /// Records the fingerprint of the fact-finding report on the dispute.
    /// Signed by the reporter role only, once, before the commit window
    /// closes — see `instructions::attest_report`.
    pub fn attest_report(ctx: Context<AttestReport>, report_hash: [u8; 32]) -> Result<()> {
        AttestReport::handle(ctx, report_hash)
    }

    /// Records the juror's hidden vote commitment during the commit window. The
    /// vote itself is absent from state until reveal — see `instructions::commit_vote`.
    pub fn commit_vote(ctx: Context<CommitVote>, commitment: [u8; 32]) -> Result<()> {
        CommitVote::handle(ctx, commitment)
    }

    /// Reveals the vote and checks it against the submitted commitment. A mismatch
    /// is rejected — see `instructions::reveal_vote`.
    pub fn reveal_vote(
        ctx: Context<RevealVote>,
        choice: Ballot,
        salt: [u8; SALT_LEN],
    ) -> Result<()> {
        RevealVote::handle(ctx, choice, salt)
    }

    /// Tallies the vote: a verdict, a one-time escalation, or the status quo.
    /// Permissionless — see `instructions::tally`.
    pub fn tally(ctx: Context<Tally>) -> Result<()> {
        Tally::handle(ctx)
    }

    /// Slashes losing votes and silence, splits the proceeds among those who were right,
    /// and releases the panel from the registry — see `instructions::settle_stakes`.
    pub fn settle_stakes<'info>(
        ctx: Context<'_, '_, 'info, 'info, SettleStakes<'info>>,
    ) -> Result<()> {
        SettleStakes::handle(ctx)
    }
}
