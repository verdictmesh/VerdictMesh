//! T016 — відбір панелі присяжних (`FR-006`, `FR-006a`).
//!
//! Юніт-тести самого відбору живуть у `src/panel.rs` — вони перевіряють, що з
//! ентропії виходить панель без повторів і що жодна позиція реєстру не буває
//! недосяжною. Тут перевіряється те, чого чиста функція перевірити не може:
//!
//! **Реєстр не можна показати вибірково.** Той, хто викликає відбір, передає
//! пари (`JurorIndex`, `Juror`) на кожен слот. Якби кількість чи порядок не
//! звірялись, вистачило б показати відбору лише зручних присяжних — і панель,
//! формально випадкова, складалася б із заздалегідь обраних.
//!
//! **Панель не можна переграти.** Ентропія прив'язана до слота, записаного при
//! відкритті спору. Повторний відбір відхиляється, а той самий спір дає ту саму
//! панель незалежно від того, коли й хто викликав.
//!
//! **Відібрані замикаються в реєстрі.** `active_disputes` піднімається саме
//! тут; без цього блокування виходу з T015 не має чого блокувати.

#[allow(dead_code)]
#[path = "harness.rs"]
mod harness;

use anchor_lang::solana_program::pubkey::Pubkey;
use harness::*;
use mollusk_svm::result::InstructionResult;
use solana_account::Account;
use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};
use verdict_mesh::{
    events::PanelSelected,
    panel::SLOT_HASHES_ID,
    state::{Dispute, DisputeState, Juror, JurorIndex, JurorRegistry, Policy},
    VerdictMeshError,
};

fn staked() -> u64 {
    usdc(100)
}

/// Реєстр удвічі більший за панель: так видно, що відбір справді вибирає, а не
/// бере всіх, хто є.
const JURORS: u32 = 6;

struct Fixture {
    jurors: Vec<Pubkey>,
    integrator: Pubkey,
    dispute: Pubkey,
    policy: Policy,
    accounts: Vec<(Address, Account)>,
    /// Пари (`JurorIndex`, `Juror`) у порядку слотів — рівно те, що інструкція
    /// очікує в `remaining_accounts`.
    registry_pairs: Vec<Pubkey>,
}

impl Fixture {
    fn new() -> Self {
        Self::with_stakes(vec![staked(); JURORS as usize])
    }

    fn with_stakes(stakes: Vec<u64>) -> Self {
        Self::build(stakes, demo_policy(), Vec::new(), ENTROPY_SLOT)
    }

    fn build(stakes: Vec<u64>, policy: Policy, panel: Vec<Pubkey>, entropy_slot: u64) -> Self {
        let count = stakes.len() as u32;
        let authority = Pubkey::new_unique();
        let (integrator, _) = integrator_pda(&authority);
        let (dispute, dispute_bump) = dispute_pda(&integrator, 0);

        let jurors: Vec<Pubkey> = (0..count).map(|_| Pubkey::new_unique()).collect();

        let mut accounts = vec![
            (
                addr(&dispute),
                dispute_for(&integrator, 0, &policy, panel, entropy_slot, dispute_bump),
            ),
            (
                addr(&registry_pda().0),
                program_account(&JurorRegistry {
                    juror_count: count,
                    bump: registry_pda().1,
                }),
            ),
            (SLOT_HASHES_ID_ADDR, slot_hashes_account()),
        ];

        let mut registry_pairs = Vec::with_capacity(2 * count as usize);
        for (slot, wallet) in jurors.iter().enumerate() {
            let slot = slot as u32;
            accounts.push((
                addr(&juror_index_pda(slot).0),
                program_account(&JurorIndex {
                    wallet: *wallet,
                    bump: juror_index_pda(slot).1,
                }),
            ));
            accounts.push((
                addr(&juror_pda(wallet).0),
                program_account(&Juror {
                    wallet: *wallet,
                    stake: stakes[slot as usize],
                    active_disputes: 0,
                    index: slot,
                    bump: juror_pda(wallet).1,
                }),
            ));
            registry_pairs.push(juror_index_pda(slot).0);
            registry_pairs.push(juror_pda(wallet).0);
        }

        Self {
            jurors,
            integrator,
            dispute,
            policy,
            accounts,
            registry_pairs,
        }
    }

    fn ix(&self) -> Instruction {
        self.ix_with(&self.registry_pairs)
    }

    /// `remaining_accounts` передаються явним списком: підміна порядку чи складу
    /// — і є тим, що перевіряють тести межі нижче.
    fn ix_with(&self, pairs: &[Pubkey]) -> Instruction {
        self.ix_for(self.dispute, pairs)
    }

    fn ix_for(&self, dispute: Pubkey, pairs: &[Pubkey]) -> Instruction {
        let mut ix = anchor_ix(
            verdict_mesh::accounts::SelectPanel {
                dispute,
                registry: registry_pda().0,
                slot_hashes: slot_hashes_id(),
            },
            verdict_mesh::instruction::SelectPanel {},
        );

        for (position, key) in pairs.iter().enumerate() {
            ix.accounts.push(AccountMeta {
                pubkey: addr(key),
                is_signer: false,
                // Записувані лише `Juror` — другі в парі: саме їм піднімається
                // `active_disputes`.
                is_writable: position % 2 == 1,
            });
        }

        ix
    }

    fn select(&self) -> InstructionResult {
        mollusk().process_instruction(&self.ix(), &self.accounts)
    }

    /// Другий спір **того самого** інтегратора над **тим самим** реєстром:
    /// різниця лише в адресі спору. Дві окремі фікстури тут нічого не довели б
    /// — у них різні присяжні, тож панелі різнились би й без будь-якої ентропії.
    fn sibling(&self, dispute_id: u64) -> (Pubkey, Vec<(Address, Account)>) {
        let (dispute, bump) = dispute_pda(&self.integrator, dispute_id);

        let mut accounts = self.accounts.clone();
        accounts.push((
            addr(&dispute),
            dispute_for(
                &self.integrator,
                dispute_id,
                &self.policy,
                Vec::new(),
                ENTROPY_SLOT,
                bump,
            ),
        ));

        (dispute, accounts)
    }

    fn panel(&self, result: &InstructionResult) -> Vec<Pubkey> {
        let dispute: Dispute = decode(resulting(result, &self.dispute));
        dispute.panel
    }
}

/// Адреса сисвара як `Address` — константа програми, взята без перекладу через
/// `addr`, бо це вже той самий 32-байтовий ключ.
const SLOT_HASHES_ID_ADDR: Address = Address::new_from_array(SLOT_HASHES_ID.to_bytes());

fn slot_hashes_id() -> Pubkey {
    SLOT_HASHES_ID
}

fn slot_hashes_account() -> Account {
    keyed_account_for_slot_hashes(&mollusk()).1
}

/// Спір для відбору: обв'язка дає стан у тому вигляді, у якому його лишає
/// `open_dispute`, а тут міняється лише те, заради чого написаний тест.
fn dispute_for(
    integrator: &Pubkey,
    dispute_id: u64,
    policy: &Policy,
    panel: Vec<Pubkey>,
    entropy_slot: u64,
    bump: u8,
) -> Account {
    let mut dispute = dispute_state(integrator, dispute_id, policy, bump);
    dispute.panel = panel;
    dispute.entropy_slot = entropy_slot;
    dispute_account(&dispute)
}

// ── що виходить ─────────────────────────────────────────────────────────────

#[test]
fn fills_the_panel_with_distinct_jurors_from_the_registry() {
    let fixture = Fixture::new();
    let result = fixture.select();
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let panel = fixture.panel(&result);
    assert_eq!(panel.len(), fixture.policy.panel_size as usize);
    assert!(panel.iter().all(|member| fixture.jurors.contains(member)));

    let mut distinct = panel.clone();
    distinct.sort();
    distinct.dedup();
    assert_eq!(distinct.len(), panel.len(), "повтор у панелі: {panel:?}");
}

/// Відбір **вибирає**, а не бере всіх: реєстр удвічі більший за панель, і хтось
/// має лишитися поза нею. Тест на «панель заповнена» без цього проходив би й
/// для інструкції, що просто переписує реєстр у спір.
#[test]
fn leaves_part_of_the_registry_out_of_the_panel() {
    let fixture = Fixture::new();
    let result = fixture.select();

    let panel = fixture.panel(&result);
    assert!(panel.len() < fixture.jurors.len());
    assert!(fixture.jurors.iter().any(|juror| !panel.contains(juror)));
}

/// Головне зчеплення з T015: відібраний присяжний замикається в реєстрі, і саме
/// цей лічильник не дасть йому вийти зі стейком до фіналізації спору.
#[test]
fn locks_exactly_the_selected_jurors_in_the_registry() {
    let fixture = Fixture::new();
    let result = fixture.select();
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let panel = fixture.panel(&result);
    for wallet in &fixture.jurors {
        let juror: Juror = decode(resulting(&result, &juror_pda(wallet).0));
        let expected = u16::from(panel.contains(wallet));
        assert_eq!(juror.active_disputes, expected, "присяжний {wallet}");
    }
}

/// Спір лишається у `Committing`: відбір панелі не є подією стан-машини, він
/// лише наповнює те, чого бракувало для голосування.
#[test]
fn leaves_the_dispute_committing() {
    let fixture = Fixture::new();
    let result = fixture.select();

    let dispute: Dispute = decode(resulting(&result, &fixture.dispute));
    assert_eq!(dispute.state, DisputeState::Committing);
    assert!(dispute.verdict.is_none());
}

// ── придатність ─────────────────────────────────────────────────────────────

/// `FR-011a`: придатність міряє політика **цього** спору. Реєстр спільний для
/// всіх інтеграторів, тож недостатній стейк — це не «не присяжний», а «не для
/// цього спору».
#[test]
fn skips_jurors_staked_below_the_policy_of_this_dispute() {
    let thin = usdc(1);
    let fixture = Fixture::with_stakes(vec![thin, staked(), thin, staked(), thin, staked()]);

    let result = fixture.select();
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let panel = fixture.panel(&result);
    assert_eq!(panel.len(), 3);
    for (slot, wallet) in fixture.jurors.iter().enumerate() {
        if slot % 2 == 0 {
            assert!(!panel.contains(wallet), "недостатній стейк у панелі");
        }
    }
}

/// Придатних менше за панель — це відмова, а не панель поменше: `FR-010` рахує
/// кворум від розміру панелі з політики, і зменшена панель тихо змінила б
/// правила розгляду.
#[test]
fn refuses_when_too_few_jurors_meet_the_policy() {
    let fixture =
        Fixture::with_stakes(vec![staked(), staked(), usdc(1), usdc(1), usdc(1), usdc(1)]);

    let result = fixture.select();
    assert!(failed_with(&result, VerdictMeshError::RegistryTooSmall));
}

// ── реєстр не можна показати вибірково ──────────────────────────────────────

/// Найдорожчий тест файлу. Показати відбору лише зручних присяжних — це панель,
/// формально випадкова, а насправді складена заздалегідь.
#[test]
fn refuses_a_registry_that_is_not_enumerated_in_full() {
    let fixture = Fixture::new();
    let short = &fixture.registry_pairs[..fixture.registry_pairs.len() - 2];

    let result = mollusk().process_instruction(&fixture.ix_with(short), &fixture.accounts);
    assert!(failed_with(&result, VerdictMeshError::InvalidPanelAccounts));
}

/// Порядок теж є частиною перелічення: слот виводиться з номера, тож пара, яку
/// поставили не на своє місце, не сходиться з адресою.
#[test]
fn refuses_a_registry_given_out_of_slot_order() {
    let fixture = Fixture::new();

    let mut shuffled = fixture.registry_pairs.clone();
    shuffled.swap(0, 2);
    shuffled.swap(1, 3);

    let result = mollusk().process_instruction(&fixture.ix_with(&shuffled), &fixture.accounts);
    assert!(failed_with(&result, VerdictMeshError::InvalidPanelAccounts));
}

/// Той самий присяжний, поданий двічі, подвоїв би його шанс потрапити в панель —
/// і, за збігу, вагу його голосу.
#[test]
fn refuses_the_same_juror_twice() {
    let fixture = Fixture::new();

    let mut duplicated = fixture.registry_pairs.clone();
    duplicated[2] = duplicated[0];
    duplicated[3] = duplicated[1];

    let result = mollusk().process_instruction(&fixture.ix_with(&duplicated), &fixture.accounts);
    assert!(failed_with(&result, VerdictMeshError::InvalidPanelAccounts));
}

/// Запис присяжного має відповідати гаманцю зі слота. Інакше підставлений
/// `Juror` із великим стейком зробив би придатним будь-кого.
#[test]
fn refuses_a_juror_record_that_does_not_match_its_slot() {
    let fixture = Fixture::new();

    let mut swapped = fixture.registry_pairs.clone();
    swapped.swap(1, 3);

    let result = mollusk().process_instruction(&fixture.ix_with(&swapped), &fixture.accounts);
    assert!(failed_with(&result, VerdictMeshError::InvalidPanelAccounts));
}

/// Записувані лише `Juror` — їм піднімається `active_disputes`. Акаунт, лише
/// читаний, з'ясувати краще до того, як половина панелі вже записана.
#[test]
fn refuses_a_juror_account_it_cannot_write_to() {
    let fixture = Fixture::new();

    let mut ix = fixture.ix();
    for meta in ix.accounts.iter_mut() {
        if meta.pubkey == addr(&juror_pda(&fixture.jurors[0]).0) {
            meta.is_writable = false;
        }
    }

    let result = mollusk().process_instruction(&ix, &fixture.accounts);
    assert!(failed_with(&result, VerdictMeshError::InvalidPanelAccounts));
}

// ── панель не можна переграти ───────────────────────────────────────────────

/// Той самий спір дає ту саму панель — це і є «детермінований відбір» з
/// `FR-006`. Без цього відбір неможливо ані перевірити, ані відтворити ззовні.
#[test]
fn gives_the_same_panel_for_the_same_dispute() {
    let fixture = Fixture::new();

    let first = fixture.select();
    let second = fixture.select();

    assert_eq!(fixture.panel(&first), fixture.panel(&second));
}

/// Різні спори — різні панелі, навіть коли ентропія взята з одного слота. Саме
/// для цього в зерно входить ідентифікатор спору.
#[test]
fn gives_different_panels_to_different_disputes() {
    let fixture = Fixture::new();
    let (sibling, accounts) = fixture.sibling(1);

    let first = fixture.panel(&fixture.select());

    let result =
        mollusk().process_instruction(&fixture.ix_for(sibling, &fixture.registry_pairs), &accounts);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);
    let second: Dispute = decode(resulting(&result, &sibling));

    // Реєстр, слот ентропії й політика ті самі — різниться лише адреса спору.
    assert_ne!(first, second.panel);
}

/// Повторний відбір відхиляється. Він дав би ту саму панель ще раз, а разом із
/// нею — другий інкремент `active_disputes` у тих самих присяжних, тобто
/// блокування виходу, яке вже ніколи не знімається.
#[test]
fn refuses_a_second_selection() {
    let fixture = Fixture::new();
    let first = fixture.select();
    assert!(first.program_result.is_ok(), "{:?}", first.raw_result);

    let mut accounts = fixture.accounts.clone();
    replace(
        &mut accounts,
        &fixture.dispute,
        resulting(&first, &fixture.dispute).clone(),
    );

    let second = mollusk().process_instruction(&fixture.ix(), &accounts);
    assert!(failed_with(&second, VerdictMeshError::PanelAlreadySelected));
}

/// Слот ентропії вийшов за глибину `SlotHashes` — відбір відмовляється, а не
/// бере чужий хеш. Мовчазний промах дав би панель, виведену не з того, з чого
/// обіцяно, і ніхто б цього не побачив.
#[test]
fn refuses_a_dispute_whose_entropy_slot_has_aged_out() {
    let fixture = Fixture::build(
        vec![staked(); JURORS as usize],
        demo_policy(),
        Vec::new(),
        SLOT - 600,
    );

    let result = fixture.select();
    assert!(failed_with(&result, VerdictMeshError::EntropyUnavailable));
}

/// Вікно подання вже закрите: панель, відібрана після нього, не має часу
/// голосувати, і спір усе одно піде в автоескалацію.
#[test]
fn refuses_a_selection_after_the_commit_window() {
    let mut policy = demo_policy();
    policy.commit_window = 1;

    let fixture = Fixture::build(
        vec![staked(); JURORS as usize],
        policy,
        Vec::new(),
        ENTROPY_SLOT,
    );

    let mut mollusk = mollusk();
    mollusk.sysvars.clock.unix_timestamp = NOW + 5;

    let result = mollusk.process_instruction(&fixture.ix(), &fixture.accounts);
    assert!(failed_with(&result, VerdictMeshError::WindowClosed));
}

// ── FR-029: подія ───────────────────────────────────────────────────────────

/// Подія несе і панель, і слот ентропії — того, хто захоче перевірити відбір
/// ззовні, це позбавляє потреби здогадуватись, з чого він виводився.
#[test]
fn emits_the_panel_together_with_its_entropy_slot() {
    let fixture = Fixture::new();
    let (mollusk, logs) = mollusk_with_logs();

    let result = mollusk.process_instruction(&fixture.ix(), &fixture.accounts);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let events: Vec<PanelSelected> = emitted(&logs);
    assert_eq!(events.len(), 1);

    let event = &events[0];
    assert_eq!(event.dispute, fixture.dispute);
    assert_eq!(event.entropy_slot, ENTROPY_SLOT);
    assert_eq!(event.panel, fixture.panel(&result));
}

// ── межа FR-006a ────────────────────────────────────────────────────────────

/// `FR-006a` — межа, яку не видно в жодному з тестів вище, бо вона про те, чого
/// в коді **немає**. Ентропія входить у програму рівно через один акаунт:
/// сисвар `SlotHashes`. Замінити джерело на VRF означає замінити цей акаунт і
/// `panel::entropy_of` — реєстр, голосування й виконання вердикту про це не
/// дізнаються.
#[test]
fn takes_entropy_through_exactly_one_account() {
    let fixture = Fixture::new();
    let ix = fixture.ix();

    let named = ix.accounts.len() - fixture.registry_pairs.len();
    assert_eq!(named, 3, "dispute, registry і джерело ентропії — і все");
    assert_eq!(ix.accounts[2].pubkey, SLOT_HASHES_ID_ADDR);
}

/// Чужий акаунт замість сисвара не приймається: інакше ентропію можна було б
/// підсунути разом із панеллю, яку вона дає.
#[test]
fn refuses_an_entropy_source_that_is_not_the_sysvar() {
    let fixture = Fixture::new();
    let impostor = Pubkey::new_unique();

    let mut ix = anchor_ix(
        verdict_mesh::accounts::SelectPanel {
            dispute: fixture.dispute,
            registry: registry_pda().0,
            slot_hashes: impostor,
        },
        verdict_mesh::instruction::SelectPanel {},
    );
    for (position, key) in fixture.registry_pairs.iter().enumerate() {
        ix.accounts.push(AccountMeta {
            pubkey: addr(key),
            is_signer: false,
            is_writable: position % 2 == 1,
        });
    }

    let mut accounts = fixture.accounts.clone();
    accounts.push((addr(&impostor), slot_hashes_account()));

    let result = mollusk().process_instruction(&ix, &accounts);
    assert!(result.program_result.is_err());
}

/// Відбір нічий: підпису не вимагає ніхто. Результат зафіксовано ще при
/// відкритті спору, тож дозволяти відбір комусь одному означало б лише дати
/// цьому комусь можливість не виконати його взагалі.
#[test]
fn needs_nobody_to_sign() {
    let fixture = Fixture::new();
    assert!(fixture.ix().accounts.iter().all(|meta| !meta.is_signer));
}

/// Спір належить своєму інтегратору: PDA виводиться з нього, тож підставлений
/// чужий спір не сходиться з адресою.
#[test]
fn refuses_a_dispute_that_does_not_match_its_integrator() {
    let fixture = Fixture::new();
    let stranger = Pubkey::new_unique();

    let mut accounts = fixture.accounts.clone();
    let mut dispute: Dispute = decode(&fixture.accounts[0].1);
    dispute.integrator = stranger;
    replace(&mut accounts, &fixture.dispute, program_account(&dispute));

    let result = mollusk().process_instruction(&fixture.ix(), &accounts);
    assert!(result.program_result.is_err());
}

/// Лічильник реєстру і переданий перелік мусять збігатися. Розбіжність — це
/// або показаний не весь реєстр, або показаний чужий: і те, й те звужує
/// вибірку до того, що зручно тому, хто викликає.
#[test]
fn refuses_a_registry_count_that_disagrees_with_the_accounts_given() {
    let fixture = Fixture::new();

    let mut accounts = fixture.accounts.clone();
    replace(
        &mut accounts,
        &registry_pda().0,
        program_account(&JurorRegistry {
            juror_count: JURORS - 1,
            bump: registry_pda().1,
        }),
    );

    let result = mollusk().process_instruction(&fixture.ix(), &accounts);
    assert!(failed_with(&result, VerdictMeshError::InvalidPanelAccounts));
}

/// Заглушка проти регресу: `Integrator` в інструкції не бере участі. Спір несе
/// знімок політики (`FR-003`), і другий шлях дізнатися правила був би другим
/// шляхом їх підмінити.
#[test]
fn keeps_the_integrator_out_of_the_instruction() {
    let fixture = Fixture::new();
    let integrator = addr(&fixture.integrator);

    assert!(fixture
        .ix()
        .accounts
        .iter()
        .all(|meta| meta.pubkey != integrator));
}
