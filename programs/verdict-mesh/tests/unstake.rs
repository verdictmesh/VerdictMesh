//! T015 — вихід із реєстру присяжних (`FR-007`).
//!
//! Дві речі, і обидві тихі, якщо зламані.
//!
//! **Блокування.** Присяжний, що бере участь у нефіналізованому спорі, не може
//! забрати стейк. Інакше програш у голосуванні коштує рівно нічого: досить
//! вийти між розкриттям і розрахунком, і слешити буде нічого.
//!
//! **Swap-remove.** Останній слот переїжджає на звільнений, `juror_count`
//! зменшується. Тут легко зробити половину роботи: переписати `JurorIndex`, але
//! не `Juror.index`, або навпаки. Розбіжність між ними не падає — вона стає
//! дірою в реєстрі, яку побачить відбір панелі (`FR-006`, T016) на живому спорі.
//! Тому окремий тест звіряє обидва боки після переїзду.

#[allow(dead_code)]
#[path = "harness.rs"]
mod harness;

use anchor_lang::solana_program::pubkey::Pubkey;
use harness::*;
use mollusk_svm::{program::keyed_account_for_system_program, result::InstructionResult};
use solana_account::Account;
use solana_address::Address;
use solana_instruction::Instruction;
use verdict_mesh::{
    events::JurorUnstaked,
    state::{Juror, JurorIndex, JurorRegistry},
    VerdictMeshError,
};

fn staked() -> u64 {
    usdc(100)
}

/// Реєстр із трьох присяжних. Троє — найменше число, за якого «вийшов
/// останній», «вийшов не останній» і «переїхав хтось третій» — різні випадки.
const JURORS: u32 = 3;

struct Fixture {
    jurors: Vec<Pubkey>,
    tokens: Vec<Pubkey>,
    mint: Pubkey,
    accounts: Vec<(Address, Account)>,
}

impl Fixture {
    fn new() -> Self {
        Self::with_active_disputes(vec![0; JURORS as usize])
    }

    /// Реєстр збирається зі стану, а не прогоном `stake` тричі: тест про вихід
    /// не має падати через щось у вступі, і навпаки.
    fn with_active_disputes(active: Vec<u16>) -> Self {
        let count = active.len() as u32;
        let mint = Pubkey::new_unique();
        let reporter = Pubkey::new_unique();

        let jurors: Vec<Pubkey> = (0..count).map(|_| Pubkey::new_unique()).collect();
        let tokens: Vec<Pubkey> = (0..count).map(|_| Pubkey::new_unique()).collect();

        let mut accounts = vec![
            (addr(&config_pda().0), config_account(&mint, &reporter)),
            (addr(&mint), settlement_mint()),
            (
                addr(&registry_pda().0),
                program_account(&JurorRegistry {
                    juror_count: count,
                    bump: registry_pda().1,
                }),
            ),
            // Сховище тримає стейки всіх трьох: вихід одного не має чіпати чужі.
            (
                addr(&stake_vault_pda().0),
                token_account(&mint, &config_pda().0, staked() * u64::from(count)),
            ),
            keyed_account_for_token_program(),
            keyed_account_for_system_program(),
            keyed_account_for_this_program(),
        ];

        for (index, wallet) in jurors.iter().enumerate() {
            accounts.push((addr(wallet), wallet_with_rent()));
            accounts.push((
                addr(&juror_pda(wallet).0),
                program_account(&Juror {
                    wallet: *wallet,
                    stake: staked(),
                    active_disputes: active[index],
                    index: index as u32,
                    bump: juror_pda(wallet).1,
                }),
            ));
            accounts.push((
                addr(&juror_index_pda(index as u32).0),
                program_account(&JurorIndex {
                    wallet: *wallet,
                    bump: juror_index_pda(index as u32).1,
                }),
            ));
            accounts.push((addr(&tokens[index]), token_account(&mint, wallet, 0)));
        }

        Self {
            jurors,
            tokens,
            mint,
            accounts,
        }
    }

    fn count(&self) -> u32 {
        self.jurors.len() as u32
    }

    fn last(&self) -> u32 {
        self.count() - 1
    }

    /// Інструкція для присяжного зі слота `index`. Хвіст реєстру передається
    /// завжди, бо закривається завжди; звільнюваний слот і переїзд — лише коли
    /// виходить не останній.
    fn ix(&self, index: u32) -> Instruction {
        let moving = index != self.last();
        self.ix_with(
            index,
            self.last(),
            moving.then(|| juror_index_pda(index).0),
            moving.then(|| juror_pda(&self.jurors[self.last() as usize]).0),
        )
    }

    fn ix_with(
        &self,
        index: u32,
        tail_slot: u32,
        vacated_index: Option<Pubkey>,
        mover: Option<Pubkey>,
    ) -> Instruction {
        let wallet = self.jurors[index as usize];
        anchor_ix(
            verdict_mesh::accounts::Unstake {
                juror: wallet,
                config: config_pda().0,
                settlement_mint: self.mint,
                registry: registry_pda().0,
                juror_account: juror_pda(&wallet).0,
                tail_index: juror_index_pda(tail_slot).0,
                vacated_index,
                mover,
                juror_tokens: self.tokens[index as usize],
                stake_vault: stake_vault_pda().0,
                token_program: TOKEN_PROGRAM,
                system_program: SYSTEM_PROGRAM,
            },
            verdict_mesh::instruction::Unstake {},
        )
    }

    fn unstake(&self, index: u32) -> InstructionResult {
        mollusk().process_instruction(&self.ix(index), &self.accounts)
    }
}

/// Гаманець присяжного вже оплатив оренду своїх акаунтів, тож на ньому лишилось
/// небагато. Точне число тут ні на що не впливає — важливо лише, що повернена
/// оренда до нього додається, а не замінює його баланс.
fn wallet_with_rent() -> Account {
    wallet(1_000_000)
}

// ── блокування ──────────────────────────────────────────────────────────────

/// `FR-007` дослівно. Без цього програш у голосуванні коштує нічого: вийти
/// можна між розкриттям і розрахунком, і слешити буде нічого.
#[test]
fn refuses_a_juror_that_still_holds_an_unfinalized_dispute() {
    let fixture = Fixture::with_active_disputes(vec![1, 0, 0]);
    let result = fixture.unstake(0);
    assert!(failed_with(&result, VerdictMeshError::JurorLocked));
}

/// Лічильник, а не прапорець: присяжний сидить у кількох панелях одночасно, і
/// вихід має відкритися лише після останньої.
#[test]
fn refuses_a_juror_that_holds_several_disputes() {
    let fixture = Fixture::with_active_disputes(vec![0, 3, 0]);
    let result = fixture.unstake(1);
    assert!(failed_with(&result, VerdictMeshError::JurorLocked));
}

// ── кошти ───────────────────────────────────────────────────────────────────

/// Стейк повертається повністю і саме зі сховища. Чужі стейки в тому ж сховищі
/// лишаються недоторканими — сховище спільне, і помилка в сумі виносила б не
/// свої гроші.
#[test]
fn returns_the_whole_stake_and_leaves_the_other_stakes_alone() {
    let fixture = Fixture::new();
    let result = fixture.unstake(0);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let tokens = token_state(resulting(&result, &fixture.tokens[0]));
    assert_eq!(tokens.amount, staked());

    let vault = token_state(resulting(&result, &stake_vault_pda().0));
    assert_eq!(vault.amount, staked() * u64::from(JURORS - 1));
}

/// Переказ зі сховища підписує PDA програми — приватного ключа до нього не
/// існує. Якби сховище лишалось під гаманцем, цей переказ був би можливий і
/// повз програму.
#[test]
fn leaves_the_vault_under_the_same_program_address() {
    let fixture = Fixture::new();
    let result = fixture.unstake(0);

    let vault = token_state(resulting(&result, &stake_vault_pda().0));
    assert_eq!(vault.owner, addr(&config_pda().0));
}

/// Запис присяжного закривається, а не лишається з нульовим стейком: акаунт із
/// живим дискримінатором і нулем усередині — це присяжний, якого відбір може
/// вибрати, і панель, яка нічим не ризикує.
#[test]
fn closes_the_juror_record() {
    let fixture = Fixture::new();
    let result = fixture.unstake(0);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let record = resulting(&result, &juror_pda(&fixture.jurors[0]).0);
    assert_eq!(record.lamports, 0);
    assert!(record.data.is_empty());
}

// ── swap-remove ─────────────────────────────────────────────────────────────

/// Головний тест файлу. Після переїзду обидва боки мають показувати те саме:
/// `JurorIndex` звільненого слота — на того, хто переїхав, і його ж `Juror.index`
/// — на цей слот. Половина роботи не падає, вона стає дірою в реєстрі.
#[test]
fn keeps_the_slot_and_the_record_in_step_after_the_move() {
    let fixture = Fixture::new();
    let mover = fixture.jurors[fixture.last() as usize];

    let result = fixture.unstake(0);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let vacated: JurorIndex = decode(resulting(&result, &juror_index_pda(0).0));
    assert_eq!(vacated.wallet, mover);

    let moved: Juror = decode(resulting(&result, &juror_pda(&mover).0));
    assert_eq!(moved.index, 0);
}

/// Хвіст закривається завжди — інакше в реєстрі лишається слот поза межею
/// лічильника, який усе ще посилається на присяжного. Відбір туди не ходить,
/// але покладатися на це означає тримати `FR-006` на дисципліні читача.
#[test]
fn closes_the_tail_slot() {
    let fixture = Fixture::new();
    let result = fixture.unstake(0);

    let tail = resulting(&result, &juror_index_pda(fixture.last()).0);
    assert_eq!(tail.lamports, 0);
    assert!(tail.data.is_empty());
}

#[test]
fn shrinks_the_registry_by_one() {
    let fixture = Fixture::new();
    let result = fixture.unstake(0);

    let registry: JurorRegistry = decode(resulting(&result, &registry_pda().0));
    assert_eq!(registry.juror_count, JURORS - 1);
}

/// Стейк і слот того, кого не чіпали, лишаються як були. Swap-remove торкається
/// рівно двох слотів, і третій присяжний не повинен цього помітити.
#[test]
fn leaves_the_untouched_juror_where_it_was() {
    let fixture = Fixture::new();
    let bystander = fixture.jurors[1];

    let result = fixture.unstake(0);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let record: Juror = decode(resulting(&result, &juror_pda(&bystander).0));
    assert_eq!(record.index, 1);
    assert_eq!(record.stake, staked());

    let slot: JurorIndex = decode(resulting(&result, &juror_index_pda(1).0));
    assert_eq!(slot.wallet, bystander);
}

/// Виходить останній — переїжджати нікому, і хвіст є його ж слотом. Той самий
/// акаунт двома входами інструкції був би двома копіями одного буфера, і
/// закриття однієї з них перезаписалося б іншою.
#[test]
fn the_last_juror_leaves_without_moving_anyone() {
    let fixture = Fixture::new();
    let last = fixture.last();

    let result = fixture.unstake(last);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let slot = resulting(&result, &juror_index_pda(last).0);
    assert_eq!(slot.lamports, 0);
    assert!(slot.data.is_empty());

    let registry: JurorRegistry = decode(resulting(&result, &registry_pda().0));
    assert_eq!(registry.juror_count, JURORS - 1);

    // Решта реєстру не зрушила.
    let untouched: Juror = decode(resulting(&result, &juror_pda(&fixture.jurors[1]).0));
    assert_eq!(untouched.index, 1);
}

/// Реєстр із одного присяжного спорожняється до нуля, а не до «одного, якого
/// немає»: наступний `stake` має отримати слот 0.
#[test]
fn empties_a_registry_of_one() {
    let fixture = Fixture::with_active_disputes(vec![0]);
    let result = fixture.unstake(0);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let registry: JurorRegistry = decode(resulting(&result, &registry_pda().0));
    assert_eq!(registry.juror_count, 0);
}

/// Реєстр після переїзду має лишатись придатним для наступного виходу — і саме
/// для того, хто щойно переїхав. Одиничний swap-remove легко зробити так, що
/// він працює рівно раз: слот показує на нового власника, а його запис — ще на
/// старий індекс, і другий вихід виносить із реєстру не того.
#[test]
fn survives_a_second_departure_by_the_juror_that_just_moved() {
    let fixture = Fixture::new();
    let moved = fixture.jurors[fixture.last() as usize];
    let bystander = fixture.jurors[1];

    let first = fixture.unstake(0);
    assert!(first.program_result.is_ok(), "{:?}", first.raw_result);

    // Стан беремо з результату цілком: тест про другий вихід не має права
    // спиратися на власне уявлення про те, що лишив по собі перший.
    let mut accounts = fixture.accounts.clone();
    for key in [
        registry_pda().0,
        stake_vault_pda().0,
        juror_index_pda(0).0,
        juror_pda(&moved).0,
        fixture.tokens[fixture.last() as usize],
    ] {
        replace(&mut accounts, &key, resulting(&first, &key).clone());
    }

    // Той, хто переїхав на слот 0, виходить наступним: тепер хвіст — слот 1.
    let ix = anchor_ix(
        verdict_mesh::accounts::Unstake {
            juror: moved,
            config: config_pda().0,
            settlement_mint: fixture.mint,
            registry: registry_pda().0,
            juror_account: juror_pda(&moved).0,
            tail_index: juror_index_pda(1).0,
            vacated_index: Some(juror_index_pda(0).0),
            mover: Some(juror_pda(&bystander).0),
            juror_tokens: fixture.tokens[fixture.last() as usize],
            stake_vault: stake_vault_pda().0,
            token_program: TOKEN_PROGRAM,
            system_program: SYSTEM_PROGRAM,
        },
        verdict_mesh::instruction::Unstake {},
    );

    let second = mollusk().process_instruction(&ix, &accounts);
    assert!(second.program_result.is_ok(), "{:?}", second.raw_result);

    let registry: JurorRegistry = decode(resulting(&second, &registry_pda().0));
    assert_eq!(registry.juror_count, 1);

    let slot: JurorIndex = decode(resulting(&second, &juror_index_pda(0).0));
    assert_eq!(slot.wallet, bystander);

    let record: Juror = decode(resulting(&second, &juror_pda(&bystander).0));
    assert_eq!(record.index, 0);

    let vault = token_state(resulting(&second, &stake_vault_pda().0));
    assert_eq!(vault.amount, staked());
}

// ── межі переїзду ───────────────────────────────────────────────────────────

/// Переїзд, якого не мало бути: виходить останній, але клієнт передав слот і
/// того, хто нібито переїжджає. Прийняти це означало б переписати чужий запис.
#[test]
fn rejects_a_move_when_the_leaver_is_already_last() {
    let fixture = Fixture::new();
    let last = fixture.last();

    let ix = fixture.ix_with(
        last,
        last,
        Some(juror_index_pda(last).0),
        Some(juror_pda(&fixture.jurors[last as usize]).0),
    );

    let result = mollusk().process_instruction(&ix, &fixture.accounts);
    assert!(failed_with(&result, VerdictMeshError::InvalidRegistryTail));
}

/// Дзеркальний випадок і найдорожчий: виходить не останній, а переїзду не
/// передали. Мовчазне прийняття лишило б у реєстрі слот, що посилається на
/// присяжного, якого там уже немає.
#[test]
fn rejects_a_missing_move_when_the_leaver_is_not_last() {
    let fixture = Fixture::new();
    let ix = fixture.ix_with(0, fixture.last(), None, None);

    let result = mollusk().process_instruction(&ix, &fixture.accounts);
    assert!(failed_with(&result, VerdictMeshError::InvalidRegistryTail));
}

/// Половина переїзду — теж переїзд, який не можна прийняти: слот без того, хто
/// в нього переїжджає, і навпаки.
#[test]
fn rejects_half_a_move() {
    let fixture = Fixture::new();
    let last = fixture.last();

    for (vacated, mover) in [
        (Some(juror_index_pda(0).0), None),
        (None, Some(juror_pda(&fixture.jurors[last as usize]).0)),
    ] {
        let ix = fixture.ix_with(0, last, vacated, mover);
        let result = mollusk().process_instruction(&ix, &fixture.accounts);
        assert!(failed_with(&result, VerdictMeshError::InvalidRegistryTail));
    }
}

/// Хвіст — це саме останній слот. Підставлений інший означав би, що закриється
/// живий слот, а справжній хвіст лишиться поза межею лічильника.
#[test]
fn rejects_a_tail_that_is_not_the_last_slot() {
    let fixture = Fixture::new();
    let ix = fixture.ix_with(
        0,
        1,
        Some(juror_index_pda(0).0),
        Some(juror_pda(&fixture.jurors[1]).0),
    );

    let result = mollusk().process_instruction(&ix, &fixture.accounts);
    assert!(result.program_result.is_err());
}

/// Переїжджає той, хто справді стоїть у хвості. Чужий запис на його місці дав
/// би присяжному індекс, за яким його немає.
#[test]
fn rejects_a_mover_that_is_not_the_juror_in_the_tail_slot() {
    let fixture = Fixture::new();
    let ix = fixture.ix_with(
        0,
        fixture.last(),
        Some(juror_index_pda(0).0),
        Some(juror_pda(&fixture.jurors[1]).0),
    );

    let result = mollusk().process_instruction(&ix, &fixture.accounts);
    assert!(result.program_result.is_err());
}

/// Звільнюваний слот — свій. Чужий на його місці був би виходом, який виносить
/// із реєстру когось іншого.
#[test]
fn rejects_a_vacated_slot_that_belongs_to_another_juror() {
    let fixture = Fixture::new();
    let ix = fixture.ix_with(
        0,
        fixture.last(),
        Some(juror_index_pda(1).0),
        Some(juror_pda(&fixture.jurors[fixture.last() as usize]).0),
    );

    let result = mollusk().process_instruction(&ix, &fixture.accounts);
    assert!(result.program_result.is_err());
}

// ── межі повноважень ────────────────────────────────────────────────────────

/// Вихід за когось іншого. Запис присяжного виводиться з підписанта, тож чужий
/// просто не сходиться з адресою.
#[test]
fn rejects_unstaking_on_behalf_of_another_juror() {
    let fixture = Fixture::new();
    let stranger = fixture.jurors[1];

    let ix = anchor_ix(
        verdict_mesh::accounts::Unstake {
            juror: fixture.jurors[0],
            config: config_pda().0,
            settlement_mint: fixture.mint,
            registry: registry_pda().0,
            juror_account: juror_pda(&stranger).0,
            tail_index: juror_index_pda(fixture.last()).0,
            vacated_index: Some(juror_index_pda(1).0),
            mover: Some(juror_pda(&fixture.jurors[fixture.last() as usize]).0),
            juror_tokens: fixture.tokens[0],
            stake_vault: stake_vault_pda().0,
            token_program: TOKEN_PROGRAM,
            system_program: SYSTEM_PROGRAM,
        },
        verdict_mesh::instruction::Unstake {},
    );

    let result = mollusk().process_instruction(&ix, &fixture.accounts);
    assert!(result.program_result.is_err());
}

#[test]
fn rejects_a_juror_that_did_not_sign() {
    let fixture = Fixture::new();

    let mut ix = fixture.ix(0);
    let juror = addr(&fixture.jurors[0]);
    for meta in ix.accounts.iter_mut() {
        if meta.pubkey == juror {
            meta.is_signer = false;
        }
    }

    let result = mollusk().process_instruction(&ix, &fixture.accounts);
    assert!(result.program_result.is_err());
}

/// Стейк повертається в розрахунковому активі й на акаунт самого присяжного.
#[test]
fn rejects_a_payout_into_someone_elses_tokens() {
    let fixture = Fixture::new();
    let stranger = Pubkey::new_unique();

    let mut accounts = fixture.accounts.clone();
    replace(
        &mut accounts,
        &fixture.tokens[0],
        token_account(&fixture.mint, &stranger, 0),
    );

    let result = mollusk().process_instruction(&fixture.ix(0), &accounts);
    assert!(result.program_result.is_err());
}

// ── FR-029: подія ───────────────────────────────────────────────────────────

/// Пари подій `JurorStaked` / `JurorUnstaked` має вистачати, щоб відтворити
/// склад реєстру повністю — тому в події є і звільнений слот, і той, хто на
/// нього переїхав.
#[test]
fn emits_the_departure_together_with_the_move() {
    let fixture = Fixture::new();
    let (mollusk, logs) = mollusk_with_logs();

    let result = mollusk.process_instruction(&fixture.ix(0), &fixture.accounts);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let events: Vec<JurorUnstaked> = emitted(&logs);
    assert_eq!(events.len(), 1);

    let event = &events[0];
    assert_eq!(event.juror, fixture.jurors[0]);
    assert_eq!(event.stake, staked());
    assert_eq!(event.index, 0);
    assert_eq!(event.moved, Some(fixture.jurors[fixture.last() as usize]));
    assert_eq!(event.juror_count, JURORS - 1);
}

/// Виходив останній — переїзду не було, і подія має це показувати, а не
/// повідомляти про переїзд сам на себе.
#[test]
fn reports_no_move_when_the_last_juror_leaves() {
    let fixture = Fixture::new();
    let (mollusk, logs) = mollusk_with_logs();

    let result = mollusk.process_instruction(&fixture.ix(fixture.last()), &fixture.accounts);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let events: Vec<JurorUnstaked> = emitted(&logs);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].moved, None);
    assert_eq!(events[0].index, fixture.last());
}
