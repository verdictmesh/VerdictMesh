use anchor_lang::prelude::*;

pub mod claims;
pub mod errors;
pub mod events;
pub mod instructions;
pub mod payout;
pub mod seeds;
pub mod state;

pub use errors::DepinError;
pub use instructions::*;
pub use state::*;

declare_id!("DaQXW5oL5bDH77XxJ4MoSvwTv6X7GwqNixkNYGNLrFk8");

/// A DePIN reward escrow that buys arbitration from VerdictMesh — the second
/// integration, built to be unlike the first (`reference_escrow`).
///
/// A network locks an epoch's reward for one of its operators, then reports
/// what share of it the operator earned. The operator either lets the report
/// stand or challenges it with a counter-offer, and the panel picks one of the
/// two offers. Whichever wins, the reward is **split** between both parties;
/// the loser's share is where the review deposit is reimbursed from, so there
/// is no bond vault. No milestones, asymmetric roles, positions that are
/// numbers rather than roles — and the same `open_dispute` CPI and
/// `Dispute::ruling` read as the milestone escrow uses.
#[program]
pub mod depin_escrow {
    use super::*;

    /// Locks the reward. Both parties sign: with the epoch the operator accepts
    /// the review policy. See `instructions::epoch`.
    pub fn fund_epoch<'info>(
        ctx: Context<'_, '_, '_, 'info, FundEpoch<'info>>,
        epoch_id: u64,
        reward: u64,
        challenge_window: i64,
    ) -> Result<()> {
        FundEpoch::handle(ctx, epoch_id, reward, challenge_window)
    }

    /// The network publishes the operator's share; the challenge window
    /// starts. See `instructions::epoch`.
    pub fn report_usage<'info>(
        ctx: Context<'_, '_, '_, 'info, ReportUsage<'info>>,
        operator_bps: u16,
    ) -> Result<()> {
        ReportUsage::handle(ctx, operator_bps)
    }

    /// Pays out an unchallenged report once its window has closed. Anyone may
    /// send it. See `instructions::epoch`.
    pub fn settle_report<'info>(
        ctx: Context<'_, '_, '_, 'info, SettleReport<'info>>,
    ) -> Result<()> {
        SettleReport::handle(ctx)
    }

    /// The operator challenges the report with a counter-offer — one CPI into
    /// VerdictMesh. See `instructions::dispute`.
    pub fn dispute_epoch<'info>(
        ctx: Context<'_, '_, '_, 'info, DisputeEpoch<'info>>,
        claimed_bps: u16,
    ) -> Result<()> {
        DisputeEpoch::handle(ctx, claimed_bps)
    }

    /// Executes the verdict: pays out the winning offer and reimburses the
    /// deposit from the loser's share. Anyone may send it. See
    /// `instructions::settle`.
    pub fn settle_dispute<'info>(
        ctx: Context<'_, '_, '_, 'info, SettleDispute<'info>>,
    ) -> Result<()> {
        SettleDispute::handle(ctx)
    }
}
