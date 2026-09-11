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

/// Вихід із реєстру присяжних — `FR-007`.
///
/// **Swap-remove, а не діра.** Останній слот переїжджає на звільнений,
/// `juror_count` зменшується на один, хвіст закривається. Без цього в реєстрі
/// накопичуються порожні слоти, і детермінований відбір (`FR-006`) або
/// промахується в них, або мусить їх обходити — тобто перестає бути O(1).
///
/// **Хвіст закривається завжди, а переїзд буває не завжди.** Коли виходить
/// останній, звільнений слот і є хвостом — переїжджати нікому. Передати той
/// самий акаунт двома входами інструкції не можна: Anchor розібрав би його у
/// дві незалежні копії, і закриття однієї перезаписалося б записом іншої. Тому
/// вхід переїзду необов'язковий, і його наявність мусить точно відповідати
/// тому, чи виходить останній — розбіжність в обидва боки відхиляється.
impl Unstake<'_> {
    pub fn handle(ctx: Context<Unstake>) -> Result<()> {
        // Перше і головне. Присяжний, що сидить у нефіналізованому спорі, не
        // може забрати стейк: інакше програш у голосуванні коштує нічого —
        // досить вийти між розкриттям і розрахунком, і слешити буде нічого.
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
            // Виходить останній: хвіст — його ж слот, і закриє його констрейнт.
            (None, None) => {
                require_eq!(index, last, VerdictMeshError::InvalidRegistryTail);
                require_keys_eq!(tail_wallet, wallet, VerdictMeshError::InvalidRegistryTail);
                None
            }
            (Some(vacated), Some(mover)) => {
                require_neq!(index, last, VerdictMeshError::InvalidRegistryTail);
                // Слот, що звільняється, має бути своїм. Чужий на його місці
                // був би виходом, який виносить із реєстру когось іншого.
                require_keys_eq!(
                    vacated.wallet,
                    wallet,
                    VerdictMeshError::InvalidRegistryTail
                );

                // Обидва боки посилання переписуються тут і поруч: слот — на
                // того, хто переїхав, і його запис — на цей слот. Половина
                // роботи не падає, вона стає дірою в реєстрі.
                vacated.wallet = tail_wallet;
                mover.index = index;
                Some(tail_wallet)
            }
            _ => return Err(VerdictMeshError::InvalidRegistryTail.into()),
        };

        ctx.accounts.registry.juror_count = last;

        // Переказ зі сховища підписує PDA програми: приватного ключа до нього
        // не існує, тож стейк виходить лише тим шляхом, який програма щойно
        // перевірила (`FR-014`).
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
    /// Виходить лише той, хто підписав: запис присяжного виводиться з цього
    /// ключа, тож за чужий вийти нічим.
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

    /// Останній слот реєстру. Закривається завжди — реєстр щоразу коротшає
    /// рівно на хвіст, і саме тому в ньому не може лишитись слота поза межею
    /// лічильника, який усе ще посилається на присяжного.
    ///
    /// Оренда повертається тому, хто виходить, хоча платив за цей слот хтось
    /// інший. Слоти однакового розміру, тож кожен присяжний вносить оренду
    /// одного `JurorIndex` і одного ж забирає — різниці немає.
    #[account(
        mut,
        close = juror,
        seeds = [seeds::JUROR_INDEX, &registry.juror_count.saturating_sub(1).to_le_bytes()],
        bump = tail_index.bump,
    )]
    pub tail_index: Account<'info, JurorIndex>,

    /// Слот, що звільняється. Відсутній, коли виходить останній — тоді це той
    /// самий акаунт, що `tail_index`, і другим входом його передавати не можна.
    #[account(
        mut,
        seeds = [seeds::JUROR_INDEX, &juror_account.index.to_le_bytes()],
        bump = vacated_index.bump,
    )]
    pub vacated_index: Option<Account<'info, JurorIndex>>,

    /// Присяжний із хвоста — той, хто переїжджає у звільнений слот. Адреса
    /// виводиться з гаманця, записаного в самому хвості, тож підставити сюди
    /// чужий запис нічим.
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
