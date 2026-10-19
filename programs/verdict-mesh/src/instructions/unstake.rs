use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

use crate::{
    errors::VerdictMeshError,
    events::JurorUnstaked,
    seeds,
    state::{Config, Juror, JurorIndex, JurorRegistry},
};

/// Leaving the juror registry — `FR-007`.
///
/// **Swap-remove, not a hole.** The last slot moves into the vacated one,
/// `juror_count` decreases by one, and the tail is closed. Without this, empty slots
/// pile up in the registry, and deterministic selection (`FR-006`) either lands on
/// them or has to skip them — i.e. it stops being O(1).
///
/// **The tail is always closed, but a move does not always happen.** When the last
/// juror leaves, the vacated slot is the tail itself — there is no one to move. The
/// same account cannot be passed as two instruction inputs: Anchor would deserialize
/// it into two independent copies, and closing one would be overwritten by the write
/// of the other. So the move inputs are optional, and their presence must exactly
/// match whether the last juror is leaving — a mismatch either way is rejected.
impl Unstake<'_> {
    pub fn handle(ctx: Context<Unstake>) -> Result<()> {
        // First and foremost. A juror sitting on an unfinalized dispute cannot withdraw
        // the stake: otherwise losing a vote costs nothing — just leave between reveal
        // and settlement, and there is nothing left to slash.
        require!(
            ctx.accounts.juror_account.active_disputes == 0,
            VerdictMeshError::JurorLocked
        );

        let wallet = ctx.accounts.juror.key();
        let index = ctx.accounts.juror_account.index;
        let stake = ctx.accounts.juror_account.stake;
        let last = ctx
            .accounts
            .registry
            .juror_count
            .checked_sub(1)
            .ok_or(VerdictMeshError::Overflow)?;

        let tail_wallet = ctx.accounts.tail_index.wallet;
        let moved = match (&mut ctx.accounts.vacated_index, &mut ctx.accounts.mover) {
            // The last juror is leaving: the tail is their own slot; the constraint closes it.
            (None, None) => {
                require_eq!(index, last, VerdictMeshError::InvalidRegistryTail);
                require_keys_eq!(tail_wallet, wallet, VerdictMeshError::InvalidRegistryTail);
                None
            }
            (Some(vacated), Some(mover)) => {
                require_neq!(index, last, VerdictMeshError::InvalidRegistryTail);
                // The vacated slot must be the signer's own. Someone else's slot
                // here would be an exit that removes another juror from the registry.
                require_keys_eq!(
                    vacated.wallet,
                    wallet,
                    VerdictMeshError::InvalidRegistryTail
                );

                // Both sides of the link are rewritten here, together: the slot to the
                // juror who moved, and their record to this slot. Half the job does not
                // fail — it becomes a hole in the registry.
                vacated.wallet = tail_wallet;
                mover.index = index;
                Some(tail_wallet)
            }
            _ => return Err(VerdictMeshError::InvalidRegistryTail.into()),
        };

        ctx.accounts.registry.juror_count = last;

        // The transfer out of the vault is signed by the program PDA: it has no
        // private key, so the stake leaves only by the path the program has just
        // verified (`FR-014`).
        let config_bump = ctx.accounts.config.bump;
        let signer_seeds: &[&[&[u8]]] = &[&[seeds::CONFIG, &[config_bump]]];

        transfer_checked(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                TransferChecked {
                    from: ctx.accounts.stake_vault.to_account_info(),
                    mint: ctx.accounts.settlement_mint.to_account_info(),
                    to: ctx.accounts.juror_tokens.to_account_info(),
                    authority: ctx.accounts.config.to_account_info(),
                },
                signer_seeds,
            ),
            stake,
            ctx.accounts.settlement_mint.decimals,
        )?;

        emit!(JurorUnstaked {
            juror: wallet,
            stake,
            index,
            moved,
            juror_count: last,
        });

        Ok(())
    }
}

#[derive(Accounts)]
pub struct Unstake<'info> {
    /// Only the signer can leave: the juror record is derived from this key, so
    /// there is no way to leave on someone else's behalf.
    #[account(mut)]
    pub juror: Signer<'info>,

    #[account(seeds = [seeds::CONFIG], bump = config.bump)]
    pub config: Account<'info, Config>,

    #[account(address = config.settlement_mint)]
    pub settlement_mint: InterfaceAccount<'info, Mint>,

    #[account(mut, seeds = [seeds::REGISTRY], bump = registry.bump)]
    pub registry: Account<'info, JurorRegistry>,

    #[account(
        mut,
        close = juror,
        seeds = [seeds::JUROR, juror.key().as_ref()],
        bump = juror_account.bump,
    )]
    pub juror_account: Account<'info, Juror>,

    /// The registry's last slot. Always closed — the registry shrinks by exactly
    /// the tail each time, which is why no slot beyond the counter can remain that
    /// still points to a juror.
    ///
    /// The rent goes to the juror who is leaving, even though someone else paid
    /// for this slot. Slots are the same size, so each juror pays rent for one
    /// `JurorIndex` and takes back rent for one — there is no difference.
    #[account(
        mut,
        close = juror,
        seeds = [seeds::JUROR_INDEX, &registry.juror_count.saturating_sub(1).to_le_bytes()],
        bump = tail_index.bump,
    )]
    pub tail_index: Account<'info, JurorIndex>,

    /// The slot being vacated. Absent when the last juror leaves — it is then the
    /// same account as `tail_index` and must not be passed as a second input.
    #[account(
        mut,
        seeds = [seeds::JUROR_INDEX, &juror_account.index.to_le_bytes()],
        bump = vacated_index.bump,
    )]
    pub vacated_index: Option<Account<'info, JurorIndex>>,

    /// The tail juror — the one who moves into the vacated slot. The address is
    /// derived from the wallet stored in the tail itself, so there is no way to
    /// substitute another juror's record here.
    #[account(
        mut,
        seeds = [seeds::JUROR, tail_index.wallet.as_ref()],
        bump = mover.bump,
    )]
    pub mover: Option<Account<'info, Juror>>,

    #[account(
        mut,
        token::mint = settlement_mint,
        token::authority = juror,
    )]
    pub juror_tokens: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        seeds = [seeds::STAKE_VAULT],
        bump,
        token::mint = settlement_mint,
        token::authority = config,
    )]
    pub stake_vault: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,

    pub system_program: Program<'info, System>,
}
