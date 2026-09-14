use anchor_lang::prelude::*;

use crate::{
    errors::VerdictMeshError,
    events::{DisputeFinalized, JurorRewarded, JurorSlashed},
    seeds,
    state::{Dispute, DisputeState, Juror, Verdict, VoteCommit, BPS_DENOMINATOR},
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
/// **Частка комісії за розгляд (`FR-011`) тут не роздається** — депозит і його
/// розподіл живуть у `vault.rs` (T021, `FR-026b`). Тут ділиться лише те, що
/// злетіло зі стейків: без цього правильний голос не приносив би нічого, а
/// сховище накопичувало б суму, якої ніхто не може забрати.
///
/// **Інструкція нічия і остання.** Дозвільний кранк без підпису; він же знімає
/// `active_disputes` — по одиниці, а не в нуль: присяжний сидить у кількох
/// панелях одночасно.
impl<'info> SettleStakes<'info> {
    pub fn handle(ctx: Context<'_, '_, 'info, 'info, SettleStakes<'info>>) -> Result<()> {
        let now = Clock::get()?.unix_timestamp;
        let dispute_key = ctx.accounts.dispute.key();
        let dispute = &mut ctx.accounts.dispute;

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

            let slashed = share_of(juror.stake, bps)?;
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

        dispute.state = DisputeState::Finalized;

        emit!(DisputeFinalized {
            dispute: dispute_key,
            verdict,
            finalized_at: now,
        });

        Ok(())
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

/// Частка від суми в базисних пунктах. Через `u128`, бо добуток `u64` на
/// десять тисяч не вміщається в `u64` — а це стейк, а не лічильник.
fn share_of(amount: u64, bps: u16) -> Result<u64> {
    let share = u128::from(amount)
        .checked_mul(u128::from(bps))
        .ok_or(VerdictMeshError::Overflow)?
        / u128::from(BPS_DENOMINATOR);

    u64::try_from(share).map_err(|_| VerdictMeshError::Overflow.into())
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
    // `remaining_accounts`: пари (`Juror`, `VoteCommit`) на кожне місце панелі,
    // у її порядку. Акаунт голосу може бути порожнім — це присяжний, який
    // відбитка не подавав.
}
