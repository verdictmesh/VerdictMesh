use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

use crate::{payout::Payout, seeds, state::Epoch};

/// The vault and both parties' token accounts. Both settlement paths pay
/// through here, so the epoch's signature exists in one place.
///
/// **Both accounts are always passed**, each bound to its party by a
/// constraint: settlement is permissionless, and letting the caller pass only
/// the winner's account would let the caller choose who the winner is.
pub struct Payees<'a, 'info> {
    pub mint: &'a InterfaceAccount<'info, Mint>,
    pub vault: &'a InterfaceAccount<'info, TokenAccount>,
    pub network_tokens: &'a InterfaceAccount<'info, TokenAccount>,
    pub operator_tokens: &'a InterfaceAccount<'info, TokenAccount>,
    pub token_program: &'a Interface<'info, TokenInterface>,
}

impl<'info> Payees<'_, 'info> {
    /// Empties the vault into the two shares, signed by the epoch's PDA. There
    /// is no private key to that address, so funds leave only by the path the
    /// program has just checked.
    pub fn pay(&self, epoch: &Epoch, authority: AccountInfo<'info>, payout: Payout) -> Result<()> {
        let epoch_id = epoch.epoch_id.to_le_bytes();
        let signer_seeds: &[&[&[u8]]] = &[&[
            seeds::EPOCH,
            epoch.network.as_ref(),
            &epoch_id,
            &[epoch.bump],
        ]];

        for (to, amount) in [
            (self.operator_tokens, payout.operator),
            (self.network_tokens, payout.network),
        ] {
            if amount == 0 {
                continue;
            }
            transfer_checked(
                CpiContext::new_with_signer(
                    self.token_program.to_account_info(),
                    TransferChecked {
                        from: self.vault.to_account_info(),
                        mint: self.mint.to_account_info(),
                        to: to.to_account_info(),
                        authority: authority.clone(),
                    },
                    signer_seeds,
                ),
                amount,
                self.mint.decimals,
            )?;
        }

        Ok(())
    }
}
