use anchor_lang::prelude::*;

pub mod errors;
pub mod events;
pub mod seeds;
pub mod state;

pub use errors::VerdictMeshError;
pub use state::*;

declare_id!("8WyWpDD1ZbkTRGG6SRcYyWxApPsHaSgWn2SWJQ8xSgxq");

/// VerdictMesh — протокол-агностичний шар вирішення спорів.
///
/// Програма не тримає коштів інтегратора: ескроу читає акаунт `Dispute` як
/// звичайний стан і сам розподіляє кошти (див. docs/PLAN.md → «вердикт витягують,
/// а не проштовхують»). Тому тут немає жодної інструкції, здатної перевести чужі
/// гроші — FR-014 є властивістю конструкції, а не обіцянкою.
#[program]
pub mod verdict_mesh {
    use super::*;

    pub fn initialize(_ctx: Context<Initialize>) -> Result<()> {
        err!(VerdictMeshError::WrongState)
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}
