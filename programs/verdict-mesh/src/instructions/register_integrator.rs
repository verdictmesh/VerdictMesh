use anchor_lang::prelude::*;

use crate::{
    seeds,
    state::{Config, Integrator, Policy},
};

/// Закріплює за інтегратором політику арбітражу — `FR-001`, `FR-002`.
///
/// Політика перевіряється тут і більше ніде: у спір вона потрапляє знімком
/// (`FR-003`), а знімок ніхто не переперевіряє. Тому `Policy::validate` — це
/// єдина межа між числами, які прислав клієнт, і арифметикою над чужими
/// коштами.
///
/// Інструкції оновлення політики немає, як і в `initialize`. Політика, яку
/// можна переставити під уже відкритими спорами, зробила б знімок у `Dispute`
/// декорацією: сторони погоджувались на одні правила, а розгляд ішов би за
/// іншими. Змінити правила можна лише реєстрацією нового інтегратора — тобто
/// на очах у сторін, які до нього ще не приєднались.
impl RegisterIntegrator<'_> {
    pub fn handle(ctx: Context<RegisterIntegrator>, policy: Policy) -> Result<()> {
        policy.validate()?;

        let integrator = &mut ctx.accounts.integrator;
        integrator.authority = ctx.accounts.authority.key();
        integrator.escrow_program = ctx.accounts.escrow_program.key();
        integrator.policy = policy;
        integrator.dispute_count = 0;
        integrator.bump = ctx.bumps.integrator;

        Ok(())
    }
}

#[derive(Accounts)]
pub struct RegisterIntegrator<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    /// PDA за ключем власника: закріпити протокол за собою можна лише власним
    /// підписом, і чужий ключ не має куди записати свою політику.
    #[account(
        init,
        payer = authority,
        space = Integrator::DISCRIMINATOR.len() + Integrator::INIT_SPACE,
        seeds = [seeds::INTEGRATOR, authority.key().as_ref()],
        bump,
    )]
    pub integrator: Account<'info, Integrator>,

    /// Політики без розрахункового активу не буває: `juror_stake` і `deposit`
    /// виражені саме в ньому (`FR-011a`). Присутність `Config` робить порядок
    /// `initialize` → `register_integrator` перевіркою, а не домовленістю.
    #[account(seeds = [seeds::CONFIG], bump = config.bump)]
    pub config: Account<'info, Config>,

    /// Програма ескроу інтегратора: за нею `open_dispute` згодом упізнаватиме,
    /// що спір відкриває саме її акаунт. Перевіряємо, що це справді програма —
    /// оновити поле нічим, тож помилка в ньому була б назавжди.
    ///
    /// CHECK: жоден тест і жодна інструкція її не викликають; перевіряється
    /// лише ознака executable.
    #[account(executable)]
    pub escrow_program: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}
