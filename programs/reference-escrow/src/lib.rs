use anchor_lang::prelude::*;

declare_id!("4iYF4WRdtuSmjTXH5fSa2ow5WrdeonEoeoY3epypfTHo");

/// Milestone-ескроу, який купує арбітраж у VerdictMesh замість того, щоб писати
/// власний. Це і демо-інтеграція для US1/US2/US4/US5, і зразок, за яким міряється
/// SC-004.
///
/// Виконання вердикту працює за pull-моделлю: `settle` читає акаунт `Dispute`,
/// перевіряє власника, стан і те, що спір належить саме цьому ескроу, після чого
/// розподіляє кошти сам. VerdictMesh при цьому не має жодного повноваження над
/// цим ескроу.
#[program]
pub mod reference_escrow {
    use super::*;

    pub fn initialize(_ctx: Context<Initialize>) -> Result<()> {
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}
