//! T014 — вступ до реєстру присяжних (`FR-007`).
//!
//! Перша інструкція проекту, яка рухає чужі кошти. Тому тести тут перевіряють
//! не «поля записались», а три речі, кожна з яких коштує грошей, якщо не
//! виконується:
//!
//! 1. **Стейк справді переїхав.** CPI переказу виконує справжня програма SPL
//!    Token усередині mollusk, і баланси читаються з байтів, які вона лишила.
//!    Запис `Juror.stake` без переказу виглядав би так само зелено.
//! 2. **Стейками не володіє жоден ключ.** Авторитет сховища — PDA програми, до
//!    якої приватного ключа не існує. Це `FR-014` у місці, де його найлегше
//!    порушити непомітно.
//! 3. **Реєстр лишається щільним і перелічуваним.** Індекс видає лічильник, а
//!    не клієнт: діра в нумерації зламала б відбір панелі (`FR-006`, T016) не
//!    зараз, а на живому спорі.

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
    events::JurorStaked,
    state::{Juror, JurorIndex, JurorRegistry},
    VerdictMeshError,
};

/// Стільки вносить присяжний, і стільки ж вимагає демо-політика — але збіг тут
/// випадковий: реєстр глобальний, порогу вступу в ньому немає.
fn staked() -> u64 {
    usdc(100)
}

/// Баланс присяжного з запасом: тести на нестачу мають падати на своїй сумі, а
/// не на тому, що грошей рівно стільки, скільки треба.
fn balance() -> u64 {
    usdc(500)
}

struct Fixture {
    juror: Pubkey,
    juror_tokens: Pubkey,
    mint: Pubkey,
    accounts: Vec<(Address, Account)>,
}

impl Fixture {
    fn new() -> Self {
        Self::with_registry(missing(), 0)
    }

    /// `slot` — індекс, під який виводиться `JurorIndex`. Він має збігатися з
    /// `juror_count` переданого реєстру; тести межі навмисно передають інший.
    fn with_registry(registry: Account, slot: u32) -> Self {
        Self::build(Pubkey::new_unique(), registry, slot)
    }

    /// Наступний присяжний **того самого** протоколу: спільні розрахунковий
    /// актив, `Config`, реєстр і сховище стейків. Новий мінт тут був би не
    /// дрібницею фікстури, а іншим протоколом — і тест про щільність реєстру
    /// перевіряв би зовсім не її.
    fn joining(&self, previous: &InstructionResult, slot: u32) -> Self {
        let mut next = Self::build(
            self.mint,
            resulting(previous, &registry_pda().0).clone(),
            slot,
        );
        replace(
            &mut next.accounts,
            &config_pda().0,
            resulting(previous, &config_pda().0).clone(),
        );
        replace(
            &mut next.accounts,
            &stake_vault_pda().0,
            resulting(previous, &stake_vault_pda().0).clone(),
        );
        next
    }

    fn build(mint: Pubkey, registry: Account, slot: u32) -> Self {
        let juror = Pubkey::new_unique();
        let juror_tokens = Pubkey::new_unique();
        let reporter = Pubkey::new_unique();

        let accounts = vec![
            (addr(&juror), wallet(10_000_000_000)),
            (addr(&config_pda().0), config_account(&mint, &reporter)),
            (addr(&mint), settlement_mint()),
            (addr(&registry_pda().0), registry),
            (addr(&juror_pda(&juror).0), missing()),
            (addr(&juror_index_pda(slot).0), missing()),
            (addr(&juror_tokens), token_account(&mint, &juror, balance())),
            (addr(&stake_vault_pda().0), missing()),
            keyed_account_for_token_program(),
            keyed_account_for_system_program(),
        ];

        Self {
            juror,
            juror_tokens,
            mint,
            accounts,
        }
    }

    fn ix(&self, amount: u64, slot: u32) -> Instruction {
        anchor_ix(
            verdict_mesh::accounts::Stake {
                juror: self.juror,
                config: config_pda().0,
                settlement_mint: self.mint,
                registry: registry_pda().0,
                juror_account: juror_pda(&self.juror).0,
                juror_index: juror_index_pda(slot).0,
                juror_tokens: self.juror_tokens,
                stake_vault: stake_vault_pda().0,
                token_program: TOKEN_PROGRAM,
                system_program: SYSTEM_PROGRAM,
            },
            verdict_mesh::instruction::Stake { amount },
        )
    }

    fn stake(&self) -> InstructionResult {
        mollusk().process_instruction(&self.ix(staked(), 0), &self.accounts)
    }

    fn swapping(&self, key: &Pubkey, account: Account) -> Vec<(Address, Account)> {
        let mut accounts = self.accounts.clone();
        replace(&mut accounts, key, account);
        accounts
    }
}

/// Підміна одного акаунта за ключем. Заміна за позицією ламалася б при кожній
/// зміні порядку в `#[derive(Accounts)]` — і мовчки, бо тест на відмову
/// однаково лишався б зеленим.
fn replace(accounts: &mut [(Address, Account)], key: &Pubkey, account: Account) {
    let key = addr(key);
    let entry = accounts
        .iter_mut()
        .find(|(candidate, _)| *candidate == key)
        .unwrap_or_else(|| panic!("{key} is not among the fixture accounts"));
    entry.1 = account;
}

// ── що записується ──────────────────────────────────────────────────────────

#[test]
fn records_the_juror_and_its_slot_in_the_registry() {
    let fixture = Fixture::new();
    let result = fixture.stake();
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let juror: Juror = decode(resulting(&result, &juror_pda(&fixture.juror).0));
    assert_eq!(juror.wallet, fixture.juror);
    assert_eq!(juror.stake, staked());
    assert_eq!(juror.index, 0);
    assert_eq!(juror.bump, juror_pda(&fixture.juror).1);

    let entry: JurorIndex = decode(resulting(&result, &juror_index_pda(0).0));
    assert_eq!(entry.wallet, fixture.juror);
    assert_eq!(entry.bump, juror_index_pda(0).1);

    let registry: JurorRegistry = decode(resulting(&result, &registry_pda().0));
    assert_eq!(registry.juror_count, 1);
    assert_eq!(registry.bump, registry_pda().1);
}

/// Свіжий присяжний нікого не тримає. Якби `active_disputes` починався не з
/// нуля, `unstake` (T015) блокувався б назавжди, і виявилось би це аж там.
#[test]
fn starts_the_juror_free_of_any_dispute() {
    let fixture = Fixture::new();
    let result = fixture.stake();

    let juror: Juror = decode(resulting(&result, &juror_pda(&fixture.juror).0));
    assert_eq!(juror.active_disputes, 0);
}

// ── кошти ───────────────────────────────────────────────────────────────────

/// Головний тест файлу. `Juror.stake` — це число, яке програма пише сама собі;
/// доказом стейку воно стає лише тоді, коли стільки ж пішло з гаманця
/// присяжного і прийшло у сховище.
#[test]
fn moves_exactly_the_staked_amount_into_the_vault() {
    let fixture = Fixture::new();
    let result = fixture.stake();
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let juror_tokens = token_state(resulting(&result, &fixture.juror_tokens));
    assert_eq!(juror_tokens.amount, balance() - staked());

    let vault = token_state(resulting(&result, &stake_vault_pda().0));
    assert_eq!(vault.amount, staked());
    assert_eq!(vault.mint, addr(&fixture.mint));
}

/// `FR-014` у найвразливішому місці. Сховище стейків тримає чужі гроші, і якби
/// його авторитетом був гаманець — будь-чий, включно з нашим, — вивести все
/// можна було б однією транзакцією повз усі перевірки програми.
#[test]
fn leaves_the_vault_under_a_program_address_that_has_no_private_key() {
    let fixture = Fixture::new();
    let result = fixture.stake();

    let vault = token_state(resulting(&result, &stake_vault_pda().0));
    assert_eq!(vault.owner, addr(&config_pda().0));

    // PDA виводиться саме тому, що лежить поза кривою: приватного ключа до
    // такої адреси не існує за побудовою.
    assert!(!config_pda().0.is_on_curve());
}

/// Переказ робить справжня програма токена, і нестача коштів зупиняє всю
/// інструкцію. Якби стейк тільки записувався, цей тест пройшов би — і в реєстрі
/// з'явився б присяжний, за яким нічого не стоїть.
#[test]
fn rejects_a_stake_larger_than_the_juror_holds() {
    let fixture = Fixture::new();
    let result = mollusk().process_instruction(&fixture.ix(balance() + 1, 0), &fixture.accounts);
    assert!(result.program_result.is_err());
}

/// Нульовий стейк — це місце в панелі, яке нічим не ризикує. Слешинг від нуля
/// теж нуль, тобто голос такого присяжного безкоштовний.
#[test]
fn rejects_a_zero_stake() {
    let fixture = Fixture::new();
    let result = mollusk().process_instruction(&fixture.ix(0, 0), &fixture.accounts);
    assert!(failed_with(&result, VerdictMeshError::InvalidAmount));
}

// ── межі активу ─────────────────────────────────────────────────────────────

/// `FR-011a`: економіка присяжних тримається на одному активі. Стейк у чужому
/// токені дав би місце в панелі за щось, чого слешинг не дістає.
#[test]
fn rejects_tokens_of_another_mint() {
    let fixture = Fixture::new();
    let other_mint = Pubkey::new_unique();

    let accounts = fixture.swapping(
        &fixture.juror_tokens,
        token_account(&other_mint, &fixture.juror, balance()),
    );

    let result = mollusk().process_instruction(&fixture.ix(staked(), 0), &accounts);
    assert!(result.program_result.is_err());
}

/// Мінт передається акаунтом, бо `transfer_checked` звіряє за ним знаки. Якби
/// його не прив'язали до `Config`, підставлений мінт із іншими знаками
/// перетворив би сто одиниць на одну соту.
#[test]
fn rejects_a_mint_that_is_not_the_settlement_asset() {
    let fixture = Fixture::new();
    let impostor = Pubkey::new_unique();

    let ix = anchor_ix(
        verdict_mesh::accounts::Stake {
            juror: fixture.juror,
            config: config_pda().0,
            settlement_mint: impostor,
            registry: registry_pda().0,
            juror_account: juror_pda(&fixture.juror).0,
            juror_index: juror_index_pda(0).0,
            juror_tokens: fixture.juror_tokens,
            stake_vault: stake_vault_pda().0,
            token_program: TOKEN_PROGRAM,
            system_program: SYSTEM_PROGRAM,
        },
        verdict_mesh::instruction::Stake { amount: staked() },
    );

    let mut accounts = fixture.accounts.clone();
    accounts.push((addr(&impostor), settlement_mint()));

    let result = mollusk().process_instruction(&ix, &accounts);
    assert!(result.program_result.is_err());
}

/// Стейк вносить той, хто підписав, і зі свого. Інакше присяжним ставав би той,
/// хто вказав чужий токен-акаунт із делегуванням на себе.
#[test]
fn rejects_tokens_that_belong_to_someone_else() {
    let fixture = Fixture::new();
    let stranger = Pubkey::new_unique();

    let accounts = fixture.swapping(
        &fixture.juror_tokens,
        token_account(&fixture.mint, &stranger, balance()),
    );

    let result = mollusk().process_instruction(&fixture.ix(staked(), 0), &accounts);
    assert!(result.program_result.is_err());
}

/// Сховище створюється першим стейком, тож підставити готове — найдешевша
/// спроба відвести кошти. Прив'язка до `Config` як авторитету відсікає її.
#[test]
fn rejects_a_vault_whose_authority_is_not_the_program() {
    let fixture = Fixture::new();
    let attacker = Pubkey::new_unique();

    let accounts = fixture.swapping(
        &stake_vault_pda().0,
        token_account(&fixture.mint, &attacker, 0),
    );

    let result = mollusk().process_instruction(&fixture.ix(staked(), 0), &accounts);
    assert!(result.program_result.is_err());
}

/// Те саме з іншого боку: сховище правильного авторитету, але в чужому активі.
/// Слешинг і виплати рахуються в розрахунковому активі, і сховище в іншому
/// зробило б їх недосяжними.
#[test]
fn rejects_a_vault_that_holds_another_asset() {
    let fixture = Fixture::new();
    let other_mint = Pubkey::new_unique();

    let accounts = fixture.swapping(
        &stake_vault_pda().0,
        token_account(&other_mint, &config_pda().0, 0),
    );

    let result = mollusk().process_instruction(&fixture.ix(staked(), 0), &accounts);
    assert!(result.program_result.is_err());
}

// ── реєстр лишається щільним ────────────────────────────────────────────────

/// Другий присяжний бере наступний слот, і робить це поверх стану, який лишив
/// перший: реєстр і сховище передаються з результату, а не збираються заново.
#[test]
fn the_next_juror_takes_the_next_slot() {
    let first = Fixture::new();
    let opening = first.stake();
    assert!(opening.program_result.is_ok(), "{:?}", opening.raw_result);

    let second = first.joining(&opening, 1);

    let result = mollusk().process_instruction(&second.ix(staked(), 1), &second.accounts);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let juror: Juror = decode(resulting(&result, &juror_pda(&second.juror).0));
    assert_eq!(juror.index, 1);

    let entry: JurorIndex = decode(resulting(&result, &juror_index_pda(1).0));
    assert_eq!(entry.wallet, second.juror);

    let registry: JurorRegistry = decode(resulting(&result, &registry_pda().0));
    assert_eq!(registry.juror_count, 2);

    // Сховище спільне: другий стейк лягає поверх першого, а не заводить друге.
    let vault = token_state(resulting(&result, &stake_vault_pda().0));
    assert_eq!(vault.amount, staked() * 2);
}

/// Реєстр створюється першим стейком і після цього лише росте. Найдорожча
/// помилка була б тихою: скинутий лічильник видав би новому присяжному чужий
/// слот, і `JurorIndex` почав би вказувати на двох.
#[test]
fn never_resets_a_registry_that_already_has_jurors() {
    let registry = program_account(&JurorRegistry {
        juror_count: 3,
        bump: registry_pda().1,
    });
    let fixture = Fixture::with_registry(registry, 3);

    let result = mollusk().process_instruction(&fixture.ix(staked(), 3), &fixture.accounts);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let juror: Juror = decode(resulting(&result, &juror_pda(&fixture.juror).0));
    assert_eq!(juror.index, 3);

    let registry: JurorRegistry = decode(resulting(&result, &registry_pda().0));
    assert_eq!(registry.juror_count, 4);
}

/// Слот видає лічильник, а не клієнт. Адреса `JurorIndex` — функція від
/// `juror_count`, тож для слота попереду лічильника просто немає адреси, за
/// якою його створити: у реєстрі не може з'явитись діра.
#[test]
fn rejects_a_slot_ahead_of_the_registry_counter() {
    let fixture = Fixture::with_registry(missing(), 1);
    let result = mollusk().process_instruction(&fixture.ix(staked(), 1), &fixture.accounts);
    assert!(result.program_result.is_err());
}

/// Один гаманець — один запис. Другий стейк тим самим ключем не має куди
/// лягти: `Juror` виводиться з гаманця, і місце вже зайняте.
#[test]
fn rejects_a_second_stake_from_the_same_wallet() {
    let fixture = Fixture::new();
    let opening = fixture.stake();
    assert!(opening.program_result.is_ok(), "{:?}", opening.raw_result);

    // Стан після першого стейку — цілком: реєстр, сховище, залишок гаманця і
    // сам запис присяжного. Друга спроба має впасти саме на записі, а не на
    // тому, що решта акаунтів виглядає так, ніби першого стейку не було.
    let mut accounts = fixture.accounts.clone();
    for key in [
        juror_pda(&fixture.juror).0,
        registry_pda().0,
        stake_vault_pda().0,
        fixture.juror_tokens,
    ] {
        replace(&mut accounts, &key, resulting(&opening, &key).clone());
    }
    accounts.push((addr(&juror_index_pda(1).0), missing()));

    let result = mollusk().process_instruction(&fixture.ix(staked(), 1), &accounts);
    assert!(result.program_result.is_err());
}

// ── підпис ──────────────────────────────────────────────────────────────────

/// Стейк списується з гаманця присяжного, тож без його підпису інструкція не
/// має права ні на переказ, ні на запис у реєстр від його імені.
#[test]
fn rejects_a_juror_that_did_not_sign() {
    let fixture = Fixture::new();

    let mut ix = fixture.ix(staked(), 0);
    let juror = addr(&fixture.juror);
    for meta in ix.accounts.iter_mut() {
        if meta.pubkey == juror {
            meta.is_signer = false;
        }
    }

    let result = mollusk().process_instruction(&ix, &fixture.accounts);
    assert!(result.program_result.is_err());
}

// ── FR-029: подія ───────────────────────────────────────────────────────────

/// Подія — те, за чим watcher (T027) дізнається про склад реєстру, не читаючи
/// усі акаунти програми. Тому перевіряємо не «подія є», а що в ній рівно те,
/// що записалось у стан.
#[test]
fn emits_the_registration() {
    let fixture = Fixture::new();
    let (mollusk, logs) = mollusk_with_logs();

    let result = mollusk.process_instruction(&fixture.ix(staked(), 0), &fixture.accounts);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let events: Vec<JurorStaked> = emitted(&logs);
    assert_eq!(events.len(), 1);

    let event = &events[0];
    assert_eq!(event.juror, fixture.juror);
    assert_eq!(event.stake, staked());
    assert_eq!(event.index, 0);
    assert_eq!(event.juror_count, 1);
}
