use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

use crate::{
    errors::VerdictMeshError,
    events::JurorStaked,
    seeds,
    state::{Config, Juror, JurorIndex, JurorRegistry},
};

/// Вступ до реєстру присяжних — `FR-007`.
///
/// **Порогу стейку тут немає навмисно.** Реєстр один на протокол, а
/// `juror_stake` живе в `Policy`, тобто в кожного інтегратора свій: єдиного
/// числа, з яким можна було б звірити внесок на вступі, не існує. Придатність
/// присяжного до конкретного спору перевіряє відбір панелі (`FR-006`, T016),
/// маючи перед очима саме ту політику, за якою спір розглядається.
///
/// **Стейк не поповнюється.** `Juror` виводиться з гаманця, тож другий стейк
/// тим самим ключем не має куди лягти — і це не обмеження, а те, що тримає
/// реєстр щільним: слот `JurorIndex` видає лічильник, і поповнення або
/// створило б другий слот на того самого присяжного, або переписало б чужий.
/// Збільшити стейк можна виходом і повторним вступом (`unstake`, T015) — доти,
/// доки присяжний не тримає жодного нефіналізованого спору.
impl Stake<'_> {
    pub fn handle(ctx: Context<Stake>, amount: u64) -> Result<()> {
        // Нульовий стейк — це місце в панелі, яке нічим не ризикує: слешинг від
        // нуля теж нуль, тобто неправильний голос стає безкоштовним.
        require!(amount > 0, VerdictMeshError::InvalidAmount);

        // Індекс беруть до інкремента: саме за цим значенням щойно вивелась
        // адреса `juror_index`, тож розійтися вони не можуть.
        let index = ctx.accounts.registry.juror_count;
        let juror_count = index.checked_add(1).ok_or(VerdictMeshError::Overflow)?;
        ctx.accounts.registry.juror_count = juror_count;
        ctx.accounts.registry.bump = ctx.bumps.registry;

        // `transfer_checked`, а не `transfer`: він звіряє мінт і знаки на боці
        // програми токена. Переказ, у якому знаки взяті з чужого мінта, — це
        // сума, помилкова на два порядки, і помітна вона стала б на виплаті.
        transfer_checked(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                TransferChecked {
                    from: ctx.accounts.juror_tokens.to_account_info(),
                    mint: ctx.accounts.settlement_mint.to_account_info(),
                    to: ctx.accounts.stake_vault.to_account_info(),
                    authority: ctx.accounts.juror.to_account_info(),
                },
            ),
            amount,
            ctx.accounts.settlement_mint.decimals,
        )?;

        let wallet = ctx.accounts.juror.key();

        let juror = &mut ctx.accounts.juror_account;
        juror.wallet = wallet;
        juror.stake = amount;
        juror.active_disputes = 0;
        juror.index = index;
        juror.bump = ctx.bumps.juror_account;

        let entry = &mut ctx.accounts.juror_index;
        entry.wallet = wallet;
        entry.bump = ctx.bumps.juror_index;

        emit!(JurorStaked {
            juror: wallet,
            stake: amount,
            index,
            juror_count,
        });

        Ok(())
    }
}

#[derive(Accounts)]
pub struct Stake<'info> {
    /// Присяжний платить оренду власних акаунтів і сам є авторитетом переказу:
    /// стейк іде зі свого гаманця, а не з чужого за дорученням.
    #[account(mut)]
    pub juror: Signer<'info>,

    #[account(seeds = [seeds::CONFIG], bump = config.bump)]
    pub config: Account<'info, Config>,

    /// Мінт передається акаунтом, бо `transfer_checked` звіряє за ним знаки.
    /// Прив'язка до `Config` робить підміну неможливою: інакше мінт із іншими
    /// знаками перетворив би сто одиниць на одну соту, і програма б цього не
    /// побачила.
    #[account(address = config.settlement_mint)]
    pub settlement_mint: InterfaceAccount<'info, Mint>,

    /// `init_if_needed` тут безпечний і потрібен: адреса реєстру фіксована,
    /// поля перевіряються констрейнтами, а хендлер нічого не скидає — він лише
    /// збільшує лічильник, тому шлях «створили» і шлях «уже було» дають той
    /// самий результат. Створювати реєстр в `initialize` означало б, що ключ,
    /// який ініціалізує протокол, обирає й мить появи реєстру.
    #[account(
        init_if_needed,
        payer = juror,
        space = JurorRegistry::DISCRIMINATOR.len() + JurorRegistry::INIT_SPACE,
        seeds = [seeds::REGISTRY],
        bump,
    )]
    pub registry: Account<'info, JurorRegistry>,

    /// `init`, а не `init_if_needed`: один гаманець — один запис у реєстрі.
    /// Другий стейк тим самим ключем має впасти тут, а не переписати індекс.
    #[account(
        init,
        payer = juror,
        space = Juror::DISCRIMINATOR.len() + Juror::INIT_SPACE,
        seeds = [seeds::JUROR, juror.key().as_ref()],
        bump,
    )]
    pub juror_account: Account<'info, Juror>,

    /// Слот у реєстрі виводиться з лічильника, а не з аргументу: для номера
    /// попереду лічильника просто немає адреси, за якою його створити, тож
    /// нумерація лишається щільною. Відбір панелі (`FR-006`) на це й спирається.
    #[account(
        init,
        payer = juror,
        space = JurorIndex::DISCRIMINATOR.len() + JurorIndex::INIT_SPACE,
        seeds = [seeds::JUROR_INDEX, &registry.juror_count.to_le_bytes()],
        bump,
    )]
    pub juror_index: Account<'info, JurorIndex>,

    #[account(
        mut,
        token::mint = settlement_mint,
        token::authority = juror,
    )]
    pub juror_tokens: InterfaceAccount<'info, TokenAccount>,

    /// Спільне сховище стейків. Авторитет — `Config`, тобто PDA програми:
    /// приватного ключа до нього не існує за побудовою, і вивести кошти можна
    /// лише тим, що програма підпише сама (`FR-014`). `Config` обрано, бо його
    /// канонічний bump уже лежить у стані — кожен майбутній переказ зі сховища
    /// підписується числом, прочитаним з акаунта, а не виведеним заново.
    #[account(
        init_if_needed,
        payer = juror,
        seeds = [seeds::STAKE_VAULT],
        bump,
        token::mint = settlement_mint,
        token::authority = config,
    )]
    pub stake_vault: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,

    pub system_program: Program<'info, System>,
}
