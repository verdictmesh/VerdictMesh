use anchor_lang::prelude::*;
use anchor_spl::token_interface::Mint;

use crate::{errors::VerdictMeshError, seeds, state::Config};

/// Creates the protocol's single global account.
///
/// `Config` is written once and has no update instruction — this is not a missing
/// feature. A key able to swap `settlement_mint` out from under already-staked
/// funds, rewrite `reporter` mid-dispute, or redirect the protocol fee to itself
/// would be exactly the key that, per `FR-014`, must not exist in the system. These
/// fields can only be changed by a program migration, i.e. in full public view
/// (docs/PLAN.md → "Data model").
impl Initialize<'_> {
    pub fn handle(ctx: Context<Initialize>, reporter: Pubkey, treasury: Pubkey) -> Result<()> {
        // The zero key is a valid address with no private key behind it. Without
        // this check a client bug would produce a `Config` in which `attest_report`
        // is unreachable forever: nothing can fix it, there is no update instruction.
        require_keys_neq!(
            reporter,
            Pubkey::default(),
            VerdictMeshError::InvalidReporter
        );
        // Same for the treasury, but the consequence differs: dispute settlement transfers
        // its share (`FR-026b`) and would fail on a token account the zero key does not
        // have. **All** of finalization would fail — including slashing and jurors
        // leaving the registry, so the dispute would be stuck forever.
        require_keys_neq!(
            treasury,
            Pubkey::default(),
            VerdictMeshError::InvalidTreasury
        );

        let config = &mut ctx.accounts.config;
        config.settlement_mint = ctx.accounts.settlement_mint.key();
        config.reporter = reporter;
        config.treasury = treasury;
        config.bump = ctx.bumps.config;

        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    /// `init` itself is the guard against re-initialization: an account that already
    /// has an owner cannot be created again.
    #[account(
        init,
        payer = payer,
        space = Config::DISCRIMINATOR.len() + Config::INIT_SPACE,
        seeds = [seeds::CONFIG],
        bump,
    )]
    pub config: Account<'info, Config>,

    /// The protocol's shared settlement asset — `FR-011a`. Its type is checked now,
    /// not on the first stake transfer: a `Config` pointing at a non-mint would break
    /// every later instruction that moves funds, and the trail would lead back here.
    ///
    /// No signature is required: the settlement asset's issuer has nothing to do
    /// with the protocol.
    pub settlement_mint: InterfaceAccount<'info, Mint>,

    pub system_program: Program<'info, System>,
}
