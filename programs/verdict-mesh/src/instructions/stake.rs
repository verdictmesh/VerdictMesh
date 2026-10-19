use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

use crate::{
    errors::VerdictMeshError,
    events::JurorStaked,
    seeds,
    state::{Config, Juror, JurorIndex, JurorRegistry},
};

/// Joining the juror registry — `FR-007`.
///
/// **There is deliberately no stake threshold here.** There is one registry per
/// protocol, while `juror_stake` lives in `Policy`, i.e. each integrator has its own:
/// no single number exists to check the deposit against on entry. A juror's
/// eligibility for a specific dispute is checked by panel selection (`FR-006`, T016),
/// which has in front of it the exact policy the dispute is governed by.
///
/// **The stake cannot be topped up.** `Juror` is derived from the wallet, so a second
/// stake from the same key has nowhere to go — and this is not a limitation but what
/// keeps the registry dense: `JurorIndex` slots are issued by a counter, and a top-up
/// would either create a second slot for the same juror or overwrite someone else's.
/// The stake can be increased by leaving and re-joining (`unstake`, T015) — as long
/// as the juror holds no unfinalized dispute.
impl Stake<'_> {
    pub fn handle(ctx: Context<Stake>, amount: u64) -> Result<()> {
        // A zero stake is a panel seat that risks nothing: slashing zero yields
        // zero, so a wrong vote becomes free.
        require!(amount > 0, VerdictMeshError::InvalidAmount);

        // The index is taken before the increment: this is exactly the value the
        // `juror_index` address was just derived from, so the two cannot diverge.
        let index = ctx.accounts.registry.juror_count;
        let juror_count = index.checked_add(1).ok_or(VerdictMeshError::Overflow)?;
        ctx.accounts.registry.juror_count = juror_count;
        ctx.accounts.registry.bump = ctx.bumps.registry;

        // `transfer_checked`, not `transfer`: it verifies the mint and decimals on
        // the token program's side. A transfer using another mint's decimals is an
        // amount off by two orders of magnitude, and it would only surface on payout.
        transfer_checked(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                TransferChecked {
                    from: ctx.accounts.juror_tokens.to_account_info(),
                    mint: ctx.accounts.settlement_mint.to_account_info(),
                    to: ctx.accounts.stake_vault.to_account_info(),
                    authority: ctx.accounts.juror.to_account_info(),
                },
            ),
            amount,
            ctx.accounts.settlement_mint.decimals,
        )?;

        let wallet = ctx.accounts.juror.key();

        let juror = &mut ctx.accounts.juror_account;
        juror.wallet = wallet;
        juror.stake = amount;
        juror.active_disputes = 0;
        juror.index = index;
        juror.bump = ctx.bumps.juror_account;

        let entry = &mut ctx.accounts.juror_index;
        entry.wallet = wallet;
        entry.bump = ctx.bumps.juror_index;

        emit!(JurorStaked {
            juror: wallet,
            stake: amount,
            index,
            juror_count,
        });

        Ok(())
    }
}

#[derive(Accounts)]
pub struct Stake<'info> {
    /// The juror pays rent for their own accounts and is the transfer authority:
    /// the stake comes from their own wallet, not from someone else's by delegation.
    #[account(mut)]
    pub juror: Signer<'info>,

    #[account(seeds = [seeds::CONFIG], bump = config.bump)]
    pub config: Account<'info, Config>,

    /// The mint is passed as an account because `transfer_checked` checks decimals
    /// against it. Binding it to `Config` makes substitution impossible: otherwise a
    /// mint with different decimals would turn a hundred units into one hundredth,
    /// and the program would not notice.
    #[account(address = config.settlement_mint)]
    pub settlement_mint: InterfaceAccount<'info, Mint>,

    /// `init_if_needed` is both safe and necessary here: the registry address is
    /// fixed, fields are checked by constraints, and the handler resets nothing — it
    /// only increments the counter, so the "just created" and "already existed" paths
    /// produce the same result. Creating the registry in `initialize` would mean the
    /// key that initializes the protocol also chooses when the registry appears.
    #[account(
        init_if_needed,
        payer = juror,
        space = JurorRegistry::DISCRIMINATOR.len() + JurorRegistry::INIT_SPACE,
        seeds = [seeds::REGISTRY],
        bump,
    )]
    pub registry: Account<'info, JurorRegistry>,

    /// `init`, not `init_if_needed`: one wallet, one registry entry. A second stake
    /// from the same key must fail here rather than overwrite the index.
    #[account(
        init,
        payer = juror,
        space = Juror::DISCRIMINATOR.len() + Juror::INIT_SPACE,
        seeds = [seeds::JUROR, juror.key().as_ref()],
        bump,
    )]
    pub juror_account: Account<'info, Juror>,

    /// The registry slot is derived from the counter, not from an argument: a number
    /// ahead of the counter simply has no address to be created at, so numbering
    /// stays dense. Panel selection (`FR-006`) relies on exactly that.
    #[account(
        init,
        payer = juror,
        space = JurorIndex::DISCRIMINATOR.len() + JurorIndex::INIT_SPACE,
        seeds = [seeds::JUROR_INDEX, &registry.juror_count.to_le_bytes()],
        bump,
    )]
    pub juror_index: Account<'info, JurorIndex>,

    #[account(
        mut,
        token::mint = settlement_mint,
        token::authority = juror,
    )]
    pub juror_tokens: InterfaceAccount<'info, TokenAccount>,

    /// The shared stake vault. Its authority is `Config`, i.e. a program PDA: by
    /// construction it has no private key, and funds can only leave through what the
    /// program signs itself (`FR-014`). `Config` was chosen because its canonical
    /// bump is already stored in state — every future transfer out of the vault is
    /// signed with a value read from the account rather than re-derived.
    #[account(
        init_if_needed,
        payer = juror,
        seeds = [seeds::STAKE_VAULT],
        bump,
        token::mint = settlement_mint,
        token::authority = config,
    )]
    pub stake_vault: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,

    pub system_program: Program<'info, System>,
}
