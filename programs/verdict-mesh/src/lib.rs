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
        Initialize::handle(ctx, reporter)
    }

    /// Закріплює за інтегратором політику арбітражу. Політика перевіряється
    /// один раз, тут, і потрапляє в кожен спір знімком — див.
    /// `instructions::register_integrator`.
    pub fn register_integrator(ctx: Context<RegisterIntegrator>, policy: Policy) -> Result<()> {
        RegisterIntegrator::handle(ctx, policy)
    }

    /// Відкриває спір над замкненим залишком. Викликає її програма ескроу
    /// власним підписом — див. `instructions::open_dispute`.
    pub fn open_dispute(
        ctx: Context<OpenDispute>,
        claimant: Pubkey,
        respondent: Pubkey,
        amount: u64,
        claimant_claim_hash: [u8; 32],
        respondent_claim_hash: [u8; 32],
    ) -> Result<()> {
        OpenDispute::handle(
            ctx,
            claimant,
            respondent,
            amount,
            claimant_claim_hash,
            respondent_claim_hash,
        )
    }

    /// Вносить стейк і додає присяжного до реєстру. Порогу вступу немає:
    /// достатність стейку визначає політика того спору, у панель якого
    /// присяжний потрапляє — див. `instructions::stake`.
    pub fn stake(ctx: Context<Stake>, amount: u64) -> Result<()> {
        Stake::handle(ctx, amount)
    }

    /// Повертає стейк і виводить присяжного з реєстру swap-remove'ом. Поки
    /// присяжний тримає нефіналізований спір, виходу немає — див.
    /// `instructions::unstake`.
    pub fn unstake(ctx: Context<Unstake>) -> Result<()> {
        Unstake::handle(ctx)
    }
}
