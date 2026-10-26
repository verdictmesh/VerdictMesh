use anchor_lang::prelude::*;
use anchor_spl::token_interface::TokenInterface;
use verdict_mesh::program::VerdictMesh;

use crate::{
    claims::claim_of,
    errors::DepinError,
    events::EpochDisputed,
    payout::Payout,
    seeds,
    state::{Epoch, EpochState, FULL_SHARE},
};

/// The operator challenges the network's report — the whole integration with
/// VerdictMesh is this one CPI into `open_dispute`, signed by the epoch's PDA.
///
/// **Final-offer arbitration.** The operator does not ask the panel to work out
/// a fair share; they put a counter-offer on the table, and the verdict picks
/// one of the two. That keeps VerdictMesh's binary verdict (`Claimant` /
/// `Respondent`) meaningful for a payout that is never all-or-nothing.
///
/// **The dispute is over the difference.** `amount` is the part of the reward
/// the two offers disagree on, not the reward: the rest is paid whichever way
/// the verdict goes, and the policy's optimistic threshold (`FR-024`) should
/// see what is actually at stake.
///
/// The VerdictMesh accounts carry the names of `open_dispute` (with
/// `claimant_tokens` for its `depositor_tokens`), the same as in
/// `reference_escrow`: a client passes `prepareDispute(...).accounts` from the
/// SDK as is.
impl<'info> DisputeEpoch<'info> {
    pub fn handle(
        ctx: Context<'_, '_, '_, 'info, DisputeEpoch<'info>>,
        claimed_bps: u16,
    ) -> Result<()> {
        let EpochState::Reported {
            operator_bps: reported_bps,
            challenge_deadline,
        } = ctx.accounts.epoch.state
        else {
            return Err(DepinError::NotReported.into());
        };
        require!(
            Clock::get()?.unix_timestamp < challenge_deadline,
            DepinError::ChallengeWindowClosed
        );
        require!(claimed_bps <= FULL_SHARE, DepinError::InvalidShare);
        // Only the operator challenges, and a challenge for less than the
        // report gives would be a gift to the network at the cost of a deposit.
        require!(claimed_bps > reported_bps, DepinError::ClaimNotAboveReport);

        let reward = ctx.accounts.epoch.reward;
        let contested =
            Payout::at(reward, claimed_bps)?.operator - Payout::at(reward, reported_bps)?.operator;
        // Rounding can make two offers equal in tokens; `open_dispute` would
        // refuse a zero amount anyway, but with somebody else's error.
        require!(contested > 0, DepinError::NothingContested);

        let epoch = &ctx.accounts.epoch;
        let epoch_key = epoch.key();
        let epoch_id = epoch.epoch_id.to_le_bytes();
        let signer_seeds: &[&[&[u8]]] = &[&[
            seeds::EPOCH,
            epoch.network.as_ref(),
            &epoch_id,
            &[epoch.bump],
        ]];

        verdict_mesh::cpi::open_dispute(
            CpiContext::new_with_signer(
                ctx.accounts.verdict_mesh_program.to_account_info(),
                verdict_mesh::cpi::accounts::OpenDispute {
                    payer: ctx.accounts.operator.to_account_info(),
                    config: ctx.accounts.config.to_account_info(),
                    settlement_mint: ctx.accounts.settlement_mint.to_account_info(),
                    depositor: ctx.accounts.operator.to_account_info(),
                    integrator: ctx.accounts.integrator.to_account_info(),
                    escrow: ctx.accounts.epoch.to_account_info(),
                    dispute: ctx.accounts.dispute.to_account_info(),
                    depositor_tokens: ctx.accounts.claimant_tokens.to_account_info(),
                    dispute_vault: ctx.accounts.dispute_vault.to_account_info(),
                    token_program: ctx.accounts.token_program.to_account_info(),
                    system_program: ctx.accounts.system_program.to_account_info(),
                },
                signer_seeds,
            ),
            epoch.operator,
            epoch.network,
            contested,
            claim_of(&epoch_key, claimed_bps),
            claim_of(&epoch_key, reported_bps),
        )?;

        // Written after the call: the address means something only once a
        // dispute exists behind it, and VerdictMesh, not this program, checked it.
        let dispute = ctx.accounts.dispute.key();
        ctx.accounts.epoch.state = EpochState::Disputed {
            dispute,
            reported_bps,
            claimed_bps,
        };

        emit!(EpochDisputed {
            epoch: epoch_key,
            dispute,
            reported_bps,
            claimed_bps,
            contested,
        });

        Ok(())
    }
}

/// The VerdictMesh accounts are deliberately **untyped**: their owner checks
/// them. The one check the escrow owes itself is that the dispute goes under
/// the policy the parties agreed to.
#[derive(Accounts)]
pub struct DisputeEpoch<'info> {
    /// Pays the review deposit (`FR-026`) and the dispute's rent.
    #[account(mut)]
    pub operator: Signer<'info>,

    #[account(
        mut,
        seeds = [seeds::EPOCH, epoch.network.as_ref(), &epoch.epoch_id.to_le_bytes()],
        bump = epoch.bump,
        has_one = operator @ DepinError::NotTheOperator,
    )]
    pub epoch: Account<'info, Epoch>,

    /// CHECK: checked by VerdictMesh; the escrow only requires it to be the
    /// policy the epoch was funded under.
    #[account(mut, constraint = integrator.key() == epoch.integrator @ DepinError::WrongIntegrator)]
    pub integrator: UncheckedAccount<'info>,

    /// CHECK: VerdictMesh's global account; checked by VerdictMesh.
    pub config: UncheckedAccount<'info>,

    /// CHECK: the protocol's settlement asset; checked by VerdictMesh.
    pub settlement_mint: UncheckedAccount<'info>,

    /// CHECK: the dispute account; derived and checked by VerdictMesh.
    #[account(mut)]
    pub dispute: UncheckedAccount<'info>,

    /// CHECK: the operator's tokens for the deposit; checked by VerdictMesh.
    #[account(mut)]
    pub claimant_tokens: UncheckedAccount<'info>,

    /// CHECK: this dispute's deposit vault; checked by VerdictMesh.
    #[account(mut)]
    pub dispute_vault: UncheckedAccount<'info>,

    pub verdict_mesh_program: Program<'info, VerdictMesh>,

    pub token_program: Interface<'info, TokenInterface>,

    pub system_program: Program<'info, System>,
}
