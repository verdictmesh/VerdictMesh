use anchor_lang::prelude::*;

use crate::errors::VerdictMeshError;

/// Знаменник часток слешингу: 10 000 bps = 100% стейку.
pub const BPS_DENOMINATOR: u16 = 10_000;

/// Стеля розміру панелі. Обмежує одразу дві речі: вектор `panel` у самому
/// `Dispute` і довжину циклів, якими його перебирають відбір, підрахунок і
/// розрахунок стейків. Панель без стелі — це інструкція, якій одного дня не
/// вистачить обчислювального бюджету, і виявиться це на живому спорі.
pub const MAX_PANEL_SIZE: u8 = 32;

/// Стеля тривалості одного вікна — 30 діб. Вікна складаються в дедлайни
/// (`opened_at + commit + reveal + appeal`), і без верхньої межі сума
/// переповнює `i64`, а спір отримує дедлайн у минулому.
pub const MAX_WINDOW: i64 = 30 * 24 * 60 * 60;

/// Копіюється у кожен спір при відкритті. Зміна політики інтегратором не впливає
/// на вже відкриті спори — FR-003.
#[derive(
    AnchorSerialize, AnchorDeserialize, InitSpace, Clone, Copy, Default, PartialEq, Eq, Debug,
)]
pub struct Policy {
    pub panel_size: u8,
    pub extended_panel_size: u8,
    pub quorum: u8,
    pub extended_quorum: u8,
    pub juror_stake: u64,
    pub slash_bps_wrong: u16,
    pub slash_bps_no_reveal: u16,
    pub commit_window: i64,
    pub reveal_window: i64,
    pub appeal_window: i64,
    pub optimistic_window: i64,
    pub deposit: u64,
    pub optimistic_threshold: u64,
}

impl Policy {
    /// Перевірка живе біля типу, а не всередині інструкції реєстрації: політика
    /// потрапляє в кошти через `Dispute`, і будь-який майбутній шлях, що
    /// створює `Policy`, має проходити тут. Перевірка, дописана в одну
    /// інструкцію, наступною інструкцією просто не викликається.
    ///
    /// Усі порушення повертають один `InvalidPolicy`. Окремий код на кожне
    /// правило нічого б не додав: клієнт однаково не може «частково
    /// зареєструватись», а рядок з номером правила Anchor кладе в лог.
    pub fn validate(&self) -> Result<()> {
        // Панель і кворум.
        require!(self.panel_size > 0, VerdictMeshError::InvalidPolicy);
        require!(
            self.extended_panel_size > self.panel_size,
            VerdictMeshError::InvalidPolicy
        );
        require!(
            self.extended_panel_size <= MAX_PANEL_SIZE,
            VerdictMeshError::InvalidPolicy
        );

        // Кворум — більшість панелі, а не «скільки встигло розкритись». Інакше
        // вердикт виносить меншість, яка просто виявилась швидшою.
        require!(
            is_majority(self.quorum, self.panel_size),
            VerdictMeshError::InvalidPolicy
        );
        require!(
            is_majority(self.extended_quorum, self.extended_panel_size),
            VerdictMeshError::InvalidPolicy
        );

        // Ескалація не має знижувати планку: розширений розгляд не закривається
        // меншою кількістю голосів, ніж вимагав початковий (FR-027).
        require!(
            self.extended_quorum >= self.quorum,
            VerdictMeshError::InvalidPolicy
        );

        // Економіка присяжного. Нульовий стейк або нульовий слешинг лишають
        // механізм на місці, але роблять неправильний голос безкоштовним.
        require!(self.juror_stake > 0, VerdictMeshError::InvalidPolicy);
        require!(self.slash_bps_wrong > 0, VerdictMeshError::InvalidPolicy);
        require!(
            self.slash_bps_no_reveal <= BPS_DENOMINATOR,
            VerdictMeshError::InvalidPolicy
        );
        // FR-008b: мовчання має коштувати дорожче за програний голос, інакше
        // нерозкриття — найдешевший спосіб не програти.
        require!(
            self.slash_bps_no_reveal > self.slash_bps_wrong,
            VerdictMeshError::InvalidPolicy
        );

        // FR-026b: оплату розгляду ділять присяжні й протокол. Нуль означає, що
        // панель працює безкоштовно.
        require!(self.deposit > 0, VerdictMeshError::InvalidPolicy);

        for window in [
            self.commit_window,
            self.reveal_window,
            self.appeal_window,
            self.optimistic_window,
        ] {
            require!(
                (1..=MAX_WINDOW).contains(&window),
                VerdictMeshError::InvalidPolicy
            );
        }

        // `optimistic_threshold` навмисно без перевірки: нуль — валідна
        // політика, яка просто вимикає оптимістичний трек, бо жодна сума не
        // опиниться нижче нуля (SPEC.md → US5).

        Ok(())
    }
}

/// Строга більшість: `2 * quorum > panel`, у `u16`, щоб добуток двох `u8` не
/// переповнився на великій панелі.
fn is_majority(quorum: u8, panel: u8) -> bool {
    quorum > 0 && quorum <= panel && u16::from(quorum) * 2 > u16::from(panel)
}

#[derive(AnchorSerialize, AnchorDeserialize, InitSpace, Clone, Copy, PartialEq, Eq, Debug)]
pub enum DisputeState {
    OptimisticPending,
    Committing,
    Revealing,
    Tallied,
    Appealed,
    Finalized,
}

/// StatusQuo — «як ніби спору не було» (FR-027a). Ескроу зобов'язаний уміти
/// розподілити кошти за цим результатом, інакше автоескалація нікуди не веде.
#[derive(AnchorSerialize, AnchorDeserialize, InitSpace, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Verdict {
    Claimant,
    Respondent,
    StatusQuo,
}

#[account]
#[derive(InitSpace)]
pub struct Config {
    pub settlement_mint: Pubkey,
    /// Єдиний привілейований ключ у системі. Може лише записати відбиток звіту —
    /// FR-017a. Інструкцій, що змінюють вердикт чи рухають кошти, для нього немає.
    pub reporter: Pubkey,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Integrator {
    pub authority: Pubkey,
    pub escrow_program: Pubkey,
    pub policy: Policy,
    pub dispute_count: u64,
    pub bump: u8,
}

/// Реєстр один на протокол, а не на інтегратора: присяжний вносить стейк раз і
/// потрапляє в панелі всіх інтеграторів. Тому порогу стейку тут немає й бути не
/// може — `juror_stake` живе в `Policy`, тобто у кожного інтегратора свій.
/// Придатність присяжного до конкретного спору перевіряє відбір панелі
/// (`FR-006`, T016), а не вступ до реєстру.
#[account]
#[derive(InitSpace)]
pub struct JurorRegistry {
    pub juror_count: u32,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Juror {
    pub wallet: Pubkey,
    /// Внесена сума, а не «достатня»: достатність визначає політика того спору,
    /// у панель якого присяжний потрапляє.
    pub stake: u64,
    /// Скільки нефіналізованих спорів тримають цього присяжного. Поки не нуль —
    /// вивести стейк не можна (`FR-007`, T015).
    pub active_disputes: u16,
    /// Місце в реєстрі. Разом із `JurorIndex` дає відбору перелічуваність:
    /// `index` веде від присяжного до слота, `JurorIndex` — назад.
    pub index: u32,
    pub bump: u8,
}

/// Дає реєстру перелічуваність за індексом — без цього детермінований відбір
/// (FR-006) не може вибрати N із M, не читаючи весь реєстр офчейн.
/// Вихід присяжного — swap-remove: останній індекс переїжджає на звільнений.
#[account]
#[derive(InitSpace)]
pub struct JurorIndex {
    pub wallet: Pubkey,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Dispute {
    pub integrator: Pubkey,
    pub dispute_id: u64,
    /// Знімок, не посилання — FR-003.
    pub policy: Policy,
    /// PDA ескроу, який відкрив спір. Ескроу звіряє це поле зі своїм адресом,
    /// перш ніж виконувати вердикт — інакше чужий спір міг би розпорядитись
    /// його коштами.
    pub escrow_ref: Pubkey,
    pub claimant: Pubkey,
    pub respondent: Pubkey,
    pub amount: u64,
    pub state: DisputeState,
    /// `max_len(0)` — не помилка і не «панель на нуль присяжних». Вектор росте
    /// до `policy.extended_panel_size`, який відомий лише в момент відкриття,
    /// тому `InitSpace` рахує тут саме 4-байтовий префікс довжини, а решту
    /// додає `Dispute::space`. Так фіксовану частину все одно рахує макрос, і
    /// нове поле не може мовчки випасти з розрахунку.
    #[max_len(0)]
    pub panel: Vec<Pubkey>,
    pub report_hash: [u8; 32],
    pub claimant_claim_hash: [u8; 32],
    pub respondent_claim_hash: [u8; 32],
    pub opened_at: i64,
    /// Слот, чий хеш дає ентропію для відбору панелі — `FR-006`. Записується
    /// при відкритті і більше не змінюється: якби відбір брав ентропію з
    /// моменту **свого** виконання, його можна було б переграти, повторюючи
    /// спробу зі слота в слот, доки панель не сподобається.
    ///
    /// Це слот **перед** тим, у якому відкрито спір: хеш поточного слота ще не
    /// існує, тож відбір у тій самій транзакції його не знайшов би.
    pub entropy_slot: u64,
    pub commit_deadline: i64,
    pub reveal_deadline: i64,
    pub appeal_deadline: i64,
    pub votes_claimant: u8,
    pub votes_respondent: u8,
    pub revealed_count: u8,
    /// Автоескалація застосовується один раз — FR-027a.
    pub escalated: bool,
    pub verdict: Option<Verdict>,
    pub settled: bool,
    pub bump: u8,
}

impl Dispute {
    /// Місце під акаунт спору: фіксована частина від `InitSpace` плюс сама
    /// панель. Розмір беруть із **розширеної** панелі, а не з початкової:
    /// автоескалація (`FR-027`) доповнює той самий вектор, а збільшити акаунт
    /// після створення нічим.
    pub fn space(extended_panel_size: u8) -> usize {
        Self::DISCRIMINATOR.len()
            + Self::INIT_SPACE
            + std::mem::size_of::<Pubkey>() * extended_panel_size as usize
    }
}

/// Відбиток голосу присяжного — `FR-008`. Створюється в вікні подання і до
/// розкриття не містить нічого, з чого можна вивести сам голос: `commitment` —
/// хеш (`crate::vote`), `choice` — `None`.
///
/// **Прапорця `revealed` тут немає навмисно**, хоча модель даних у `PLAN.md`
/// його називає. Він завжди дублював би `choice.is_some()`, а два поля, які
/// зобов'язані збігатись, рано чи пізно розходяться: розрахунок стейків (T020)
/// відрізняє нерозкритий голос від розкритого і мусить робити це за одним
/// джерелом. Ним і є `choice`.
#[account]
#[derive(InitSpace)]
pub struct VoteCommit {
    pub dispute: Pubkey,
    pub juror: Pubkey,
    pub commitment: [u8; 32],
    /// `None`, доки голос не розкрито (T018). Порожнє значення — це і є
    /// «присяжний подав відбиток, але не розкрився» для `FR-008b`.
    pub choice: Option<Verdict>,
    pub bump: u8,
}
