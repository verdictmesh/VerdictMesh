use anchor_lang::prelude::*;

pub mod errors;
pub mod events;
pub mod instructions;
pub mod seeds;
pub mod state;

pub use errors::VerdictMeshError;
pub use instructions::*;
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

    /// Разова ініціалізація протоколу: розрахунковий актив і ключ ролі
    /// reporter. Інструкції, що змінює записане, у програмі немає — див.
    /// `instructions::initialize`.
    pub fn initialize(ctx: Context<Initialize>, reporter: Pubkey) -> Result<()> {
        instructions::initialize::handler(ctx, reporter)
    }
}
