use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};
use verdict_mesh::{state::Dispute, Ruling};

use crate::{
    errors::DepinError,
    events::EpochSettled,
    instructions::pay::Payees,
    payout::Payout,
    seeds,
    state::{Epoch, EpochState},
};

/// Executes a verdict over the epoch — `FR-012`, `FR-013`.
///
/// **The verdict is pulled.** `Dispute::ruling` (VerdictMesh's helper, the one
/// every integrator calls) checks that the dispute is over this epoch, has a
/// verdict, is not under appeal and is past its appeal window. The one check
/// it cannot make is ours: the dispute is the one recorded in the epoch's
/// state. That record also makes a second payout impossible — after it the
/// state is `Settled` and there is nothing to match.
///
/// **The verdict picks an offer, and both parties are paid.**
///
/// - The operator won: their offer stands, and the network — against whom the
///   verdict went — reimburses the review deposit out of its own share
///   (`FR-026a`), as far as that share goes (`FR-026f`). This is why the escrow
///   needs no bond vault: unlike a milestone deal, the loser here always has a
///   share to take from.
/// - The network won: its report stands, and the deposit has already paid for
///   the review.
/// - `StatusQuo` — the extended panel failed too (`FR-027a`): "as if there had
///   been no dispute", and without the dispute the report would have been paid
///   after its window. The operator loses the deposit, nothing else. The epoch
///   does not reopen for another challenge: a status quo that could be
///   re-disputed would let the operator hold the network's share hostage for
///   the price of one deposit per round.
///
/// Permissionless and unsigned, as `SC-005` requires: no privileged person in
/// the loop, and the direction of the payout cannot be steered from outside.
impl<'info> SettleDispute<'info> {
    pub fn handle(ctx: Context<'_, '_, '_, 'info, SettleDispute<'info>>) -> Result<()> {
        let now = Clock::get()?.unix_timestamp;
        let epoch_key = ctx.accounts.epoch.key();
        let dispute_key = ctx.accounts.dispute.key();

        let Ruling {
            winner, deposit, ..
        } = ctx.accounts.dispute.ruling(&epoch_key, now)?;

        let EpochState::Disputed {
            dispute,
            reported_bps,
            claimed_bps,
        } = ctx.accounts.epoch.state
        else {
            return Err(DepinError::NotUnderThisDispute.into());
        };
        require_keys_eq!(dispute, dispute_key, DepinError::NotUnderThisDispute);

        let reward = ctx.accounts.epoch.reward;
        let (operator_bps, payout) = match winner {
            Some(winner) if winner == ctx.accounts.epoch.operator => (
                claimed_bps,
                // From the dispute's snapshot of the policy, not the
                // integrator's current one (`FR-003`): that is what was paid.
                Payout::at(reward, claimed_bps)?.reimbursing_operator(deposit)?,
            ),
            Some(winner) if winner == ctx.accounts.epoch.network => {
                (reported_bps, Payout::at(reward, reported_bps)?)
            }
            // Unreachable by construction — the epoch's own record proves this
            // program opened the dispute, with only its own parties. A silent
            // branch here would guess the recipient of someone else's funds.
            Some(_) => return Err(DepinError::VerdictNamesAStranger.into()),
            None => (reported_bps, Payout::at(reward, reported_bps)?),
        };

        ctx.accounts.payees().pay(
            &ctx.accounts.epoch,
            ctx.accounts.epoch.to_account_info(),
            payout,
        )?;
        ctx.accounts.epoch.state = EpochState::Settled { operator_bps };

        emit!(EpochSettled {
            epoch: epoch_key,
            dispute: Some(dispute_key),
            operator_bps,
            operator_amount: payout.operator,
            network_amount: payout.network,
            reimbursed: payout.reimbursed,
        });

        Ok(())
    }

    fn payees(&self) -> Payees<'_, 'info> {
        Payees {
            mint: &self.mint,
            vault: &self.vault,
            network_tokens: &self.network_tokens,
            operator_tokens: &self.operator_tokens,
            token_program: &self.token_program,
        }
    }
}

#[derive(Accounts)]
pub struct SettleDispute<'info> {
    #[account(
        mut,
        seeds = [seeds::EPOCH, epoch.network.as_ref(), &epoch.epoch_id.to_le_bytes()],
        bump = epoch.bump,
    )]
    pub epoch: Box<Account<'info, Epoch>>,

    /// Typed with VerdictMesh's type, so Anchor checks the owner: a `Dispute`
    /// laid out by another program cannot write itself a verdict. Read-only —
    /// the escrow changes nothing in someone else's state.
    pub dispute: Box<Account<'info, Dispute>>,

    #[account(address = epoch.mint)]
    pub mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(mut, token::mint = mint, token::authority = epoch.network)]
    pub network_tokens: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(mut, token::mint = mint, token::authority = epoch.operator)]
    pub operator_tokens: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        seeds = [seeds::EPOCH_VAULT, epoch.key().as_ref()],
        bump,
        token::mint = mint,
        token::authority = epoch,
    )]
    pub vault: Box<InterfaceAccount<'info, TokenAccount>>,

    pub token_program: Interface<'info, TokenInterface>,
}
