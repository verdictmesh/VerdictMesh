use anchor_lang::prelude::*;

use crate::{
    errors::VerdictMeshError,
    events::PanelSelected,
    panel::{self, SLOT_HASHES_ID},
    seeds,
    state::{Dispute, DisputeState, Juror, JurorIndex, JurorRegistry},
};

/// Відбір панелі присяжних — `FR-006`.
///
/// **Чому це окрема інструкція, а не частина `open_dispute`.** Спір відкриває
/// **програма ескроу інтегратора**, своїм підписом. Якби відбір жив усередині,
/// кожен інтегратор мусив би прокидати через свій CPI увесь реєстр присяжних —
/// тобто знати про внутрішній устрій VerdictMesh рівно те, чого «протокол-
/// агностичний шар» знати не дає. `SC-004` міряє інтеграцію в сорока рядках, і
/// прокидання реєстру не влізло б у них само по собі.
///
/// `FR-006` вимагає відбору **в межах тієї самої транзакції**, і це лишається
/// правдою: SDK складає `open_dispute` і `select_panel` в одну транзакцію
/// (T035). Програма ж гарантує сильнішу річ, ніж «в одній транзакції»:
/// ентропія прив'язана до `entropy_slot`, записаного при відкритті, тож панель
/// визначена в момент відкриття спору й **не залежить від того, коли і хто
/// викличе відбір**. Переграти її, повторюючи спробу зі слота в слот, не можна.
///
/// **Інструкція нічия.** Підпису не вимагаємо: результат детермінований і вже
/// зафіксований, тож дозволяти його комусь одному означало б лише дати цьому
/// комусь можливість не виконати відбір узагалі.
impl<'info> SelectPanel<'info> {
    /// Сигнатура з явними лайфтаймами не косметика: `remaining_accounts` мусять
    /// жити стільки ж, скільки самі акаунти, інакше `Account::try_from` до них
    /// не застосувати.
    pub fn handle(ctx: Context<'_, '_, 'info, 'info, SelectPanel<'info>>) -> Result<()> {
        let dispute_key = ctx.accounts.dispute.key();
        let dispute = &mut ctx.accounts.dispute;

        require!(
            dispute.state == DisputeState::Committing,
            VerdictMeshError::WrongState
        );
        // Порожня панель — і є ознакою того, що відбір ще не робили. Другий
        // відбір мусить впасти: він дав би ту саму панель ще раз, а разом із
        // нею — другий інкремент `active_disputes` у тих самих присяжних, тобто
        // блокування виходу, яке ніколи не знімається.
        require!(
            dispute.panel.is_empty(),
            VerdictMeshError::PanelAlreadySelected
        );
        // Панель, відібрана після закриття вікна подання, не має часу голосувати.
        require!(
            Clock::get()?.unix_timestamp < dispute.commit_deadline,
            VerdictMeshError::WindowClosed
        );

        let policy = dispute.policy;
        let registry_size = ctx.accounts.registry.juror_count;

        // Реєстр передається цілком і в порядку слотів. Інакше придатність
        // нікуди не звірити: відбір мусить бачити всіх, кого міг би вибрати,
        // а не тих, кого вирішив показати той, хто викликає.
        let mut candidates = Vec::with_capacity(registry_size as usize);
        require_eq!(
            ctx.remaining_accounts.len(),
            2 * registry_size as usize,
            VerdictMeshError::InvalidPanelAccounts
        );

        for slot in 0..registry_size {
            let pair = 2 * slot as usize;
            let entry = enumerated_slot(&ctx.remaining_accounts[pair], slot)?;
            let juror = juror_of(&ctx.remaining_accounts[pair + 1], &entry.wallet)?;

            // `FR-011a`: придатність міряється політикою **цього** спору. Реєстр
            // спільний для всіх інтеграторів, тож єдиного порогу не існує — і
            // саме тому вступ до реєстру його не перевіряє (T014).
            if juror.stake >= policy.juror_stake {
                candidates.push((slot, juror));
            }
        }

        require!(
            candidates.len() >= policy.panel_size as usize,
            VerdictMeshError::RegistryTooSmall
        );

        let entropy = panel::entropy_of(
            &ctx.accounts.slot_hashes,
            dispute.entropy_slot,
            &dispute_key,
        )?;
        let drawn = panel::draw(
            &entropy,
            u32::try_from(candidates.len()).map_err(|_| VerdictMeshError::Overflow)?,
            policy.panel_size,
        )?;

        for position in drawn {
            let (slot, juror) = &mut candidates[position as usize];
            let info = &ctx.remaining_accounts[2 * *slot as usize + 1];

            // Лічильник, який згодом не дасть присяжному вийти з реєстру, поки
            // спір не фіналізовано (`FR-007`, T015). Без цього інкремента
            // блокування виходу не має чого блокувати.
            juror.active_disputes = juror
                .active_disputes
                .checked_add(1)
                .ok_or(VerdictMeshError::Overflow)?;

            dispute.panel.push(juror.wallet);
            store(info, juror)?;
        }

        emit!(PanelSelected {
            dispute: dispute_key,
            panel: dispute.panel.clone(),
            entropy_slot: dispute.entropy_slot,
        });

        Ok(())
    }
}

/// `JurorIndex`, який справді стоїть у слоті `slot`.
///
/// Це і є те, заради чого `JurorIndex` існує. Адреса слота — функція від його
/// номера, тож звірка адреси доводить одразу дві речі: що переданий акаунт
/// належить саме цьому слоту і що реєстр перелічено без пропусків і повторів.
/// Без цього той, хто викликає, показав би відбору лише зручних йому присяжних.
fn enumerated_slot<'info>(info: &'info AccountInfo<'info>, slot: u32) -> Result<JurorIndex> {
    let entry: Account<JurorIndex> = Account::try_from(info)?;
    expect_address(
        info,
        &[seeds::JUROR_INDEX, &slot.to_le_bytes(), &[entry.bump]],
    )?;

    Ok(entry.into_inner())
}

/// `Juror` того гаманця, що записаний у слоті реєстру. Звірка адреси йде за
/// гаманцем зі **слота**, а не з самого запису: інакше підставлений `Juror`
/// підтверджував би сам себе.
///
/// Акаунт має бути записуваним — відбір підніме йому `active_disputes`, і
/// з'ясувати, що записати нікуди, краще тут, ніж посеред запису панелі.
fn juror_of<'info>(info: &'info AccountInfo<'info>, wallet: &Pubkey) -> Result<Juror> {
    let juror: Account<Juror> = Account::try_from(info)?;
    require!(info.is_writable, VerdictMeshError::InvalidPanelAccounts);
    expect_address(info, &[seeds::JUROR, wallet.as_ref(), &[juror.bump]])?;

    Ok(juror.into_inner())
}

/// Звірка адреси з PDA, виведеною за **збереженим** bump'ом. Дешевше за
/// `find_program_address` і рівно так само однозначно: невірний bump дає іншу
/// адресу, а вона не збігається з переданою.
fn expect_address(info: &AccountInfo, seeds: &[&[u8]]) -> Result<()> {
    let expected = Pubkey::create_program_address(seeds, &crate::ID)
        .map_err(|_| error!(VerdictMeshError::InvalidPanelAccounts))?;
    require_keys_eq!(*info.key, expected, VerdictMeshError::InvalidPanelAccounts);

    Ok(())
}

/// Записує змінений стан присяжного назад в акаунт. Anchor сам робить це лише
/// для полів контексту — `remaining_accounts` лишаються на совісті інструкції.
fn store(info: &AccountInfo, juror: &Juror) -> Result<()> {
    let mut data = info.try_borrow_mut_data()?;
    juror.try_serialize(&mut &mut data[..])?;

    Ok(())
}

#[derive(Accounts)]
pub struct SelectPanel<'info> {
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

    #[account(seeds = [seeds::REGISTRY], bump = registry.bump)]
    pub registry: Account<'info, JurorRegistry>,

    /// Джерело ентропії — `FR-006`. Читається сирими байтами: 512 записів
    /// сисвара не десеріалізують цілком (див. `panel::entropy_of`).
    ///
    /// CHECK: перевіряється за адресою сисвара; вміст розбирає `panel`.
    #[account(address = SLOT_HASHES_ID)]
    pub slot_hashes: UncheckedAccount<'info>,
    // `remaining_accounts`: пари (`JurorIndex`, `Juror`) на кожен слот реєстру,
    // у порядку слотів. Типізованими полями їх не оголосити — кількість відома
    // лише в момент виклику.
}
