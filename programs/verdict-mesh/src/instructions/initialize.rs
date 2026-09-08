use anchor_lang::prelude::*;
use anchor_spl::token_interface::Mint;

use crate::{errors::VerdictMeshError, seeds, state::Config};

/// Створює єдиний глобальний акаунт протоколу.
///
/// `Config` записується один раз і не має інструкції оновлення — це не забутий
/// функціонал. Ключ, здатний переставити `settlement_mint` під уже застейкані
/// кошти або переписати `reporter` посеред спору, був би саме тим ключем, якого
/// за `FR-014` у системі бути не повинно. Змінити ці два поля можна лише
/// міграцією програми, тобто на очах у всіх (docs/PLAN.md → «Модель даних»).
impl Initialize<'_> {
    pub fn handle(ctx: Context<Initialize>, reporter: Pubkey) -> Result<()> {
        // Нульовий ключ — валідна адреса, приватного ключа до якої не існує. Без
        // цієї перевірки помилка клієнта дала б `Config`, у якому `attest_report`
        // недосяжний назавжди: виправити нема чим, інструкції оновлення немає.
        require_keys_neq!(
            reporter,
            Pubkey::default(),
            VerdictMeshError::InvalidReporter
        );

        let config = &mut ctx.accounts.config;
        config.settlement_mint = ctx.accounts.settlement_mint.key();
        config.reporter = reporter;
        config.bump = ctx.bumps.config;

        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    /// `init` тут і є захистом від повторної ініціалізації: акаунт, що вже має
    /// власника, до створення не допускається.
    #[account(
        init,
        payer = payer,
        space = Config::DISCRIMINATOR.len() + Config::INIT_SPACE,
        seeds = [seeds::CONFIG],
        bump,
    )]
    pub config: Account<'info, Config>,

    /// Спільний розрахунковий актив протоколу — `FR-011a`. Тип перевіряється
    /// зараз, а не на першому переказі стейку: `Config`, що вказує на не-мінт,
    /// зламав би кожну наступну інструкцію з коштами, і слід вів би сюди.
    ///
    /// Підпису не вимагаємо: емітент розрахункового активу до протоколу
    /// стосунку не має.
    pub settlement_mint: InterfaceAccount<'info, Mint>,

    pub system_program: Program<'info, System>,
}
