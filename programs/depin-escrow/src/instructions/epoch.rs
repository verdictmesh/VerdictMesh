use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};
use verdict_mesh::state::{Config, Integrator};

use crate::{
    errors::DepinError,
    events::{EpochFunded, EpochSettled, UsageReported},
    instructions::pay::Payees,
    payout::Payout,
    seeds,
    state::{Epoch, EpochState, FULL_SHARE},
};

/// Funding an epoch — the whole reward is locked before the work is served.
///
/// **The operator signs.** Not to receive money, but because with the epoch
/// they accept the **review policy**: a challenge of the report will be
/// judged by the panel, windows and slashing of the `Integrator` the network
/// chose.
///
/// **The policy and the asset are checked here, not at dispute time.** An
/// `Integrator` pointing at another escrow program could not open a dispute
/// from this one, and a reward outside the settlement asset could not
/// reimburse a deposit — both would surface only when the funds are already
/// locked and there is nowhere to turn.
impl<'info> FundEpoch<'info> {
    pub fn handle(
        ctx: Context<'_, '_, '_, 'info, FundEpoch<'info>>,
        epoch_id: u64,
        reward: u64,
        challenge_window: i64,
    ) -> Result<()> {
        let network = ctx.accounts.network.key();
        let operator = ctx.accounts.operator.key();
        require_keys_neq!(network, operator, DepinError::InvalidParties);
        require!(reward > 0, DepinError::InvalidReward);
        require!(challenge_window > 0, DepinError::InvalidWindow);
        require_keys_eq!(
            ctx.accounts.integrator.escrow_program,
            crate::ID,
            DepinError::WrongArbitrationProgram
        );
        require_keys_eq!(
            ctx.accounts.mint.key(),
            ctx.accounts.config.settlement_mint,
            DepinError::WrongRewardMint
        );

        transfer_checked(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                TransferChecked {
                    from: ctx.accounts.network_tokens.to_account_info(),
                    mint: ctx.accounts.mint.to_account_info(),
                    to: ctx.accounts.vault.to_account_info(),
                    authority: ctx.accounts.network.to_account_info(),
                },
            ),
            reward,
            ctx.accounts.mint.decimals,
        )?;

        let epoch = &mut ctx.accounts.epoch;
        epoch.network = network;
        epoch.operator = operator;
        epoch.mint = ctx.accounts.mint.key();
        epoch.integrator = ctx.accounts.integrator.key();
        epoch.reward = reward;
        epoch.challenge_window = challenge_window;
        epoch.epoch_id = epoch_id;
        epoch.state = EpochState::Funded;
        epoch.bump = ctx.bumps.epoch;

        emit!(EpochFunded {
            epoch: epoch.key(),
            network,
            operator,
            integrator: epoch.integrator,
            reward,
            challenge_window,
        });

        Ok(())
    }
}

/// The network reports how much of the reward the operator earned. Once, and
/// only the network: it ran the epoch and holds the usage data. The challenge
/// window starts now.
impl<'info> ReportUsage<'info> {
    pub fn handle(
        ctx: Context<'_, '_, '_, 'info, ReportUsage<'info>>,
        operator_bps: u16,
    ) -> Result<()> {
        let epoch = &mut ctx.accounts.epoch;
        require!(
            epoch.state == EpochState::Funded,
            DepinError::AlreadyReported
        );
        require!(operator_bps <= FULL_SHARE, DepinError::InvalidShare);

        let challenge_deadline = Clock::get()?
            .unix_timestamp
            .checked_add(epoch.challenge_window)
            .ok_or(DepinError::Overflow)?;
        epoch.state = EpochState::Reported {
            operator_bps,
            challenge_deadline,
        };

        emit!(UsageReported {
            epoch: epoch.key(),
            operator_bps,
            challenge_deadline,
        });

        Ok(())
    }
}

/// Pays out a report nobody challenged. Permissionless and unsigned: after
/// the window the split is fixed, and anyone may crank it — waiting for the
/// network to do so would let the payer sit on the operator's share.
impl<'info> SettleReport<'info> {
    pub fn handle(ctx: Context<'_, '_, '_, 'info, SettleReport<'info>>) -> Result<()> {
        let EpochState::Reported {
            operator_bps,
            challenge_deadline,
        } = ctx.accounts.epoch.state
        else {
            return Err(DepinError::NotReported.into());
        };
        require!(
            Clock::get()?.unix_timestamp >= challenge_deadline,
            DepinError::ChallengeWindowOpen
        );

        let payout = Payout::at(ctx.accounts.epoch.reward, operator_bps)?;
        ctx.accounts.payees().pay(
            &ctx.accounts.epoch,
            ctx.accounts.epoch.to_account_info(),
            payout,
        )?;
        ctx.accounts.epoch.state = EpochState::Settled { operator_bps };

        emit!(EpochSettled {
            epoch: ctx.accounts.epoch.key(),
            dispute: None,
            operator_bps,
            operator_amount: payout.operator,
            network_amount: payout.network,
            reimbursed: 0,
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
#[instruction(epoch_id: u64)]
pub struct FundEpoch<'info> {
    #[account(mut)]
    pub network: Signer<'info>,

    /// Signs with the network: they accept the policy they will be judged by.
    pub operator: Signer<'info>,

    /// Typed with VerdictMesh's type, so Anchor checks the owner as well — a
    /// made-up `Integrator` does not pass.
    pub integrator: Account<'info, Integrator>,

    /// Needed only for `settlement_mint`. Created once at a fixed address, so
    /// no other account of this type owned by VerdictMesh exists.
    pub config: Account<'info, Config>,

    pub mint: InterfaceAccount<'info, Mint>,

    #[account(
        init,
        payer = network,
        space = Epoch::DISCRIMINATOR.len() + Epoch::INIT_SPACE,
        seeds = [seeds::EPOCH, network.key().as_ref(), &epoch_id.to_le_bytes()],
        bump,
    )]
    pub epoch: Account<'info, Epoch>,

    #[account(mut, token::mint = mint, token::authority = network)]
    pub network_tokens: InterfaceAccount<'info, TokenAccount>,

    /// The reward vault, owned by the epoch's PDA.
    #[account(
        init,
        payer = network,
        seeds = [seeds::EPOCH_VAULT, epoch.key().as_ref()],
        bump,
        token::mint = mint,
        token::authority = epoch,
    )]
    pub vault: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ReportUsage<'info> {
    pub network: Signer<'info>,

    #[account(
        mut,
        seeds = [seeds::EPOCH, epoch.network.as_ref(), &epoch.epoch_id.to_le_bytes()],
        bump = epoch.bump,
        has_one = network @ DepinError::NotTheNetwork,
    )]
    pub epoch: Account<'info, Epoch>,
}

#[derive(Accounts)]
pub struct SettleReport<'info> {
    #[account(
        mut,
        seeds = [seeds::EPOCH, epoch.network.as_ref(), &epoch.epoch_id.to_le_bytes()],
        bump = epoch.bump,
    )]
    pub epoch: Account<'info, Epoch>,

    #[account(address = epoch.mint)]
    pub mint: InterfaceAccount<'info, Mint>,

    #[account(mut, token::mint = mint, token::authority = epoch.network)]
    pub network_tokens: InterfaceAccount<'info, TokenAccount>,

    #[account(mut, token::mint = mint, token::authority = epoch.operator)]
    pub operator_tokens: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        seeds = [seeds::EPOCH_VAULT, epoch.key().as_ref()],
        bump,
        token::mint = mint,
        token::authority = epoch,
    )]
    pub vault: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,
}
