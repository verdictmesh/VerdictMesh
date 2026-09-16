use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    close_account, transfer_checked, CloseAccount, Mint, TokenAccount, TokenInterface,
    TransferChecked,
};

use crate::{
    errors::VerdictMeshError,
    events::{DisputeFeeSettled, DisputeFinalized, JurorRewarded, JurorSlashed},
    seeds,
    state::{Config, Dispute, DisputeState, Juror, Verdict, VoteCommit},
    vault::{self, FeeSplit},
};

/// Розрахунок стейків панелі — `FR-011`, `FR-008b`, `FR-027b`.
///
/// **Слешинг рухає запис, а не токени.** Сховище стейків спільне для всього
/// протоколу, тож переказувати з нього нічого не треба й не можна: досить
/// зменшити `Juror.stake` слешеного і збільшити `Juror.stake` тих, кому це
/// дісталось. Сума записів від цього не росте, а сховище лишається не меншим за
/// неї — саме та рівність, на яку спирається `unstake` (T015), повертаючи рівно
/// `Juror.stake`. Списання зі сховища без зменшення запису дало б слешеному
/// вийти за повною сумою, тобто за чужий стейк.
///
/// **Мовчання не буває безкоштовним, і це рішення ширше за букву `FR-008b`.**
/// Вимога говорить про присяжного, який подав відбиток і не розкрив його. Але
/// той, хто не подав відбитка **взагалі**, ухилився так само — і дешевше, бо
/// буквальне читання лишило б його непокараним. Тоді найдешевшим способом не
/// голосувати за невигідне стало б не голосувати зовсім, і `FR-008b` не боронив
/// би нічого. Тому обидва випадки — одна частка, `slash_bps_no_reveal`.
///
/// **За статус-кво не слешать за «неправильний» голос.** Ніхто не голосував за
/// нього і не міг: це наслідок невдалої ескалації (`FR-027a`), а не бюлетень.
/// Порівнювати з ним чиїсь голоси немає сенсу, і карати за те, що панель не
/// зійшлась, — теж. Мовчання карається й тут: саме воно ескалацію й спричинило
/// (`FR-027b`).
///
/// **Оплата розгляду роздається тут же, і це одна інструкція навмисно.**
/// Депозит (`FR-026`) ділиться між тими самими присяжними, яких щойно перебрав
/// слешинг, — множина «хто був правий» рахується один раз і не може розійтися
/// між двома кранками. Друга причина важливіша за першу: окрема інструкція
/// мала б власне вікно, у якому спір уже фіналізований, а комісія ще ні, і
/// порядок двох дозвільних викликів вирішував би, чи дістанеться присяжному
/// заробіток. Математика ділення — у `vault.rs`.
///
/// **Комісія приходить присяжному тим самим записом, що й злетіле.** Різниця
/// лише в тому, що під неї треба справді перевести токени: злетіле вже лежить
/// у сховищі стейків, а депозит — у сховищі спору. Тому частка присяжних
/// переказується `dispute_vault → stake_vault` рівно тією сумою, на яку зросте
/// сума записів, і рівність «сховище ≥ сума записів» не хитається.
///
/// **Інструкція остання і дозвільна.** Підписати її може будь-хто; єдине, що
/// дає підпис, — оренду закритого сховища спору. Це не привілей, а причина
/// взагалі викликати кранк, без якої спір лишався б нефіналізованим, доки
/// комусь не стане цікаво. Він же знімає `active_disputes` — по одиниці, а не в
/// нуль: присяжний сидить у кількох панелях одночасно.
impl<'info> SettleStakes<'info> {
    pub fn handle(ctx: Context<'_, '_, 'info, 'info, SettleStakes<'info>>) -> Result<()> {
        let now = Clock::get()?.unix_timestamp;
        let dispute_key = ctx.accounts.dispute.key();
        // Читання, а не `&mut`: єдина зміна в самому спорі — перехід у
        // `Finalized` в кінці, а незакрита мутабельна позика не дала б передати
        // решту акаунтів у переказ комісії.
        let dispute = &ctx.accounts.dispute;

        require!(
            dispute.state == DisputeState::Tallied,
            VerdictMeshError::WrongState
        );
        // Розрахувати до кінця вікна апеляції означає розрахувати вердикт, який
        // ще можуть перекрити.
        require!(now >= dispute.appeal_deadline, VerdictMeshError::WindowOpen);

        let verdict = dispute.verdict.ok_or(VerdictMeshError::WrongState)?;
        let policy = dispute.policy;

        // Панель передається цілком, парами (`Juror`, `VoteCommit`) у порядку
        // панелі. Показати розрахунку лише зручних присяжних означало б не
        // слешити решту й лишити їх замкненими в реєстрі.
        require_eq!(
            ctx.remaining_accounts.len(),
            2 * dispute.panel.len(),
            VerdictMeshError::InvalidSettlementAccounts
        );

        let mut jurors = Vec::with_capacity(dispute.panel.len());
        let mut pot: u64 = 0;

        for (seat, wallet) in dispute.panel.iter().enumerate() {
            let pair = 2 * seat;
            let mut juror = panel_member(&ctx.remaining_accounts[pair], wallet)?;
            let choice = revealed_choice(&ctx.remaining_accounts[pair + 1], &dispute_key, wallet)?;

            let bps = match choice {
                // Розкрився і збігся — або розкрився там, де збігатися немає з
                // чим (статус-кво). Обидва зробили роботу.
                Some(choice) if verdict == Verdict::StatusQuo || choice == verdict => 0,
                Some(_) => policy.slash_bps_wrong,
                None => policy.slash_bps_no_reveal,
            };

            let slashed = vault::share_of(juror.stake, bps)?;
            juror.stake = juror
                .stake
                .checked_sub(slashed)
                .ok_or(VerdictMeshError::Overflow)?;
            pot = pot.checked_add(slashed).ok_or(VerdictMeshError::Overflow)?;

            // Те, заради чого відбір узагалі піднімав лічильник (T016). Одиниця,
            // а не нуль: обнулення випустило б присяжного з реєстру з чужого,
            // ще не розв'язаного спору (`FR-007`).
            juror.active_disputes = juror.active_disputes.saturating_sub(1);

            if slashed > 0 {
                emit!(JurorSlashed {
                    dispute: dispute_key,
                    juror: *wallet,
                    amount: slashed,
                    no_reveal: choice.is_none(),
                });
            }

            jurors.push((seat, juror, choice.is_some() && bps == 0));
        }

        // Кому дістається злетіле. За справжнього вердикту — тим, чий голос
        // збігся; за статус-кво — усім, хто розкрився: збігатися немає з чим, а
        // роботу вони зробили. Якщо не лишилось нікого, зібране просто осідає у
        // сховищі — вивести його нікому й нічим, бо інструкції, що дає комусь
        // владу над коштами, у програмі немає (`FR-014`).
        let winners = jurors.iter().filter(|(_, _, won)| *won).count();

        // Оплата розгляду — `FR-026b`. Ділиться те, що справді лежить у сховищі
        // спору, а частка присяжних рахується від оголошеної ціни: нестача,
        // якщо колись виникне, має падати на протокол, а не на присяжного.
        let fee = vault::split_fee(
            ctx.accounts.dispute_vault.amount,
            policy.deposit,
            winners > 0,
        )?;
        Self::pay_fee(&ctx, &fee)?;

        // Комісія доливається в те саме відро, що й злетіле зі стейків: обидві
        // суми дістаються тим самим людям за той самий розгляд, і рахувати їх
        // окремо означало б двічі ділити з залишком — тобто загубити копійку
        // там, де вона нічия.
        pot = pot
            .checked_add(fee.jurors)
            .ok_or(VerdictMeshError::Overflow)?;

        if winners > 0 {
            let winners = u64::try_from(winners).map_err(|_| VerdictMeshError::Overflow)?;
            let each = pot / winners;
            // Залишок від ділення віддається першому за місцем у панелі, а не
            // лишається пилом: за сотні розглядів пил стає сумою, яка не
            // належить нікому і не сходиться зі сховищем.
            let mut extra = pot % winners;

            for (_, juror, _) in jurors.iter_mut().filter(|(_, _, won)| *won) {
                let reward = each.checked_add(extra).ok_or(VerdictMeshError::Overflow)?;
                extra = 0;
                juror.stake = juror
                    .stake
                    .checked_add(reward)
                    .ok_or(VerdictMeshError::Overflow)?;

                if reward > 0 {
                    emit!(JurorRewarded {
                        dispute: dispute_key,
                        juror: juror.wallet,
                        amount: reward,
                    });
                }
            }
        }

        for (seat, juror, _) in &jurors {
            store(&ctx.remaining_accounts[2 * seat], juror)?;
        }

        ctx.accounts.dispute.state = DisputeState::Finalized;

        emit!(DisputeFeeSettled {
            dispute: dispute_key,
            jurors: fee.jurors,
            protocol: fee.protocol,
        });

        emit!(DisputeFinalized {
            dispute: dispute_key,
            verdict,
            finalized_at: now,
        });

        Ok(())
    }

    /// Виводить оплату розгляду зі сховища спору і закриває його.
    ///
    /// Сховище закривається **завжди**, навіть коли ділити не було чого:
    /// порожній токен-акаунт на кожен спір — це оренда, замкнена назавжди, і
    /// накопичується вона рівно з тією швидкістю, з якою протокол працює.
    fn pay_fee(
        ctx: &Context<'_, '_, 'info, 'info, SettleStakes<'info>>,
        fee: &FeeSplit,
    ) -> Result<()> {
        let config_bump = ctx.accounts.config.bump;
        let signer_seeds: &[&[&[u8]]] = &[&[seeds::CONFIG, &[config_bump]]];
        let decimals = ctx.accounts.settlement_mint.decimals;

        let pay = |to: AccountInfo<'info>, amount: u64| -> Result<()> {
            if amount == 0 {
                return Ok(());
            }

            transfer_checked(
                CpiContext::new_with_signer(
                    ctx.accounts.token_program.to_account_info(),
                    TransferChecked {
                        from: ctx.accounts.dispute_vault.to_account_info(),
                        mint: ctx.accounts.settlement_mint.to_account_info(),
                        to,
                        authority: ctx.accounts.config.to_account_info(),
                    },
                    signer_seeds,
                ),
                amount,
                decimals,
            )
        };

        // Частка присяжних переїжджає у сховище стейків, бо саме там живуть їхні
        // баланси: `Juror.stake` — це запис проти того сховища, і збільшити його,
        // не долив токенів, означало б пообіцяти більше, ніж є.
        pay(ctx.accounts.stake_vault.to_account_info(), fee.jurors)?;
        pay(ctx.accounts.treasury_tokens.to_account_info(), fee.protocol)?;

        close_account(CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            CloseAccount {
                account: ctx.accounts.dispute_vault.to_account_info(),
                destination: ctx.accounts.crank.to_account_info(),
                authority: ctx.accounts.config.to_account_info(),
            },
            signer_seeds,
        ))
    }
}

/// `Juror` того гаманця, що справді сидить на цьому місці панелі. Звірка йде за
/// адресою: підставлений запис прийняв би слешинг замість того, кому він
/// призначений.
fn panel_member<'info>(info: &'info AccountInfo<'info>, wallet: &Pubkey) -> Result<Juror> {
    let juror: Account<Juror> = Account::try_from(info)?;
    require!(
        info.is_writable,
        VerdictMeshError::InvalidSettlementAccounts
    );
    expect_address(info, &[seeds::JUROR, wallet.as_ref(), &[juror.bump]])?;

    Ok(juror.into_inner())
}

/// Голос присяжного, якщо він його розкрив.
///
/// Порожній акаунт — не помилка: це присяжний, який відбитка не подавав. Саме
/// тому адреса тут виводиться заново, а не звіряється за збереженим bump'ом —
/// у неіснуючого акаунта його немає, а довести, що переданий порожній акаунт
/// **і є** тим самим голосом, треба однаково.
fn revealed_choice<'info>(
    info: &'info AccountInfo<'info>,
    dispute: &Pubkey,
    wallet: &Pubkey,
) -> Result<Option<Verdict>> {
    let (expected, _) = Pubkey::find_program_address(
        &[seeds::VOTE, dispute.as_ref(), wallet.as_ref()],
        &crate::ID,
    );
    require_keys_eq!(
        *info.key,
        expected,
        VerdictMeshError::InvalidSettlementAccounts
    );

    if info.data_is_empty() {
        return Ok(None);
    }

    let vote: Account<VoteCommit> = Account::try_from(info)?;

    Ok(vote.choice.map(|choice| choice.verdict()))
}

/// Записує змінений стан присяжного назад в акаунт: Anchor робить це сам лише
/// для полів контексту, `remaining_accounts` — на совісті інструкції.
fn store(info: &AccountInfo, juror: &Juror) -> Result<()> {
    let mut data = info.try_borrow_mut_data()?;
    juror.try_serialize(&mut &mut data[..])?;

    Ok(())
}

/// Звірка адреси з PDA за **збереженим** bump'ом — дешевше за пошук і рівно так
/// само однозначно.
fn expect_address(info: &AccountInfo, seeds: &[&[u8]]) -> Result<()> {
    let expected = Pubkey::create_program_address(seeds, &crate::ID)
        .map_err(|_| error!(VerdictMeshError::InvalidSettlementAccounts))?;
    require_keys_eq!(
        *info.key,
        expected,
        VerdictMeshError::InvalidSettlementAccounts
    );

    Ok(())
}

#[derive(Accounts)]
pub struct SettleStakes<'info> {
    #[account(
        mut,
        seeds = [
            seeds::DISPUTE,
            dispute.integrator.as_ref(),
            &dispute.dispute_id.to_le_bytes(),
        ],
        bump = dispute.bump,
    )]
    pub dispute: Account<'info, Dispute>,

    /// Хто завгодно. Підпис потрібен лише тому, що оренду закритого сховища
    /// спору треба комусь віддати, і найчесніший отримувач — той, хто взяв на
    /// себе виклик кранка.
    #[account(mut)]
    pub crank: Signer<'info>,

    #[account(seeds = [seeds::CONFIG], bump = config.bump)]
    pub config: Account<'info, Config>,

    #[account(address = config.settlement_mint)]
    pub settlement_mint: InterfaceAccount<'info, Mint>,

    /// Сховище цього спору. Закривається тут — тому й `mut`.
    #[account(
        mut,
        seeds = [seeds::DISPUTE_VAULT, dispute.key().as_ref()],
        bump,
        token::mint = settlement_mint,
        token::authority = config,
    )]
    pub dispute_vault: InterfaceAccount<'info, TokenAccount>,

    /// Сюди переїжджає частка присяжних: `Juror.stake` — запис проти цього
    /// сховища, і зростати він може лише разом із ним.
    #[account(
        mut,
        seeds = [seeds::STAKE_VAULT],
        bump,
        token::mint = settlement_mint,
        token::authority = config,
    )]
    pub stake_vault: InterfaceAccount<'info, TokenAccount>,

    /// Токен-акаунт скарбниці протоколу — `FR-026b`. Перевіряється за власником
    /// із `Config`, а не за адресою самого акаунта: адреса ATA виводиться з
    /// власника й мінта, обидва вже прив'язані, а зберігати її окремо означало б
    /// друге джерело того самого факту.
    #[account(
        mut,
        token::mint = settlement_mint,
        token::authority = config.treasury,
    )]
    pub treasury_tokens: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,
    // `remaining_accounts`: пари (`Juror`, `VoteCommit`) на кожне місце панелі,
    // у її порядку. Акаунт голосу може бути порожнім — це присяжний, який
    // відбитка не подавав.
}
