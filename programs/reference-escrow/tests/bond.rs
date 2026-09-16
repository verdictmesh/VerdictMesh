//! T023a — застава за розгляд з обох сторін (`FR-026a`, `FR-026e`, `FR-026f`).
//!
//! **Тут перевіряється одна обіцянка: розгляд оплачує програвша сторона, і
//! оплачує рівно один раз.** Виконати її в ескроу «переможець забирає віху»
//! нічим, поки джерелом лишається предмет спору: програвший не отримує нічого,
//! з чого можна утримати, а переможець відшкодовував би собі сам. Тому джерело
//! винесене з предмета спору — застава, яку обидві сторони вносять при
//! укладанні угоди.
//!
//! **Гроші рахуються наскрізь, а не по інструкціях.** Угода тут створюється
//! справжньою `create_escrow`, а не викладається фікстурою: твердження про
//! заставу — це рівність між тим, скільки замкнено при укладанні, і тим,
//! скільки вийшло при закритті віхи. Викласти касу руками означало б перевіряти
//! власне уявлення про неї.

#[allow(dead_code)]
#[path = "harness.rs"]
mod harness;

use anchor_lang::solana_program::pubkey::Pubkey;
use harness::*;
use mollusk_svm::{program::keyed_account_for_system_program, result::InstructionResult};
use reference_escrow::{
    events::{EscrowOpened, MilestoneSettled},
    state::{Escrow, MilestoneState},
    EscrowError,
};
use solana_account::Account;
use solana_address::Address;
use solana_instruction::Instruction;
use verdict_mesh::state::{DisputeState, Verdict};

const DEAL: u64 = 7;
const DISPUTED: u8 = 1;

fn milestones() -> Vec<u64> {
    vec![usdc(10), usdc(20), usdc(30)]
}

fn total() -> u64 {
    milestones().iter().sum()
}

/// Скільки застави має кожна сторона на початку — з запасом на депозит.
const FUNDED: u64 = 100;

/// Застава однієї віхи з одного боку. Дорівнює депозиту демо-політики: поразка
/// має коштувати рівно стільки, скільки коштував розгляд.
fn bond() -> u64 {
    demo_policy().deposit
}

/// Скільки замикається з кожного боку при укладанні — по заставі на віху.
fn bonds_per_side() -> u64 {
    bond() * milestones().len() as u64
}

struct Fixture {
    buyer: Pubkey,
    seller: Pubkey,
    mint: Pubkey,
    settlement_mint: Pubkey,
    integrator: Pubkey,
    escrow: Pubkey,
    vault: Pubkey,
    bond_vault: Pubkey,
    buyer_tokens: Pubkey,
    seller_tokens: Pubkey,
    buyer_bond_tokens: Pubkey,
    seller_bond_tokens: Pubkey,
    accounts: Vec<(Address, Account)>,
}

impl Fixture {
    fn new() -> Self {
        Self::with_settlement_mint(None)
    }

    /// `settlement_mint` — актив, у якому пропонується застава. `None` означає
    /// «той, що в `Config`»; чужий мінт потрібен рівно одному тесту.
    fn with_settlement_mint(offered: Option<Pubkey>) -> Self {
        let buyer = Pubkey::new_unique();
        let seller = Pubkey::new_unique();
        let authority = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        let protocol_mint = Pubkey::new_unique();
        let settlement_mint = offered.unwrap_or(protocol_mint);
        let buyer_tokens = Pubkey::new_unique();
        let seller_tokens = Pubkey::new_unique();
        let buyer_bond_tokens = Pubkey::new_unique();
        let seller_bond_tokens = Pubkey::new_unique();

        let (integrator, _) = mesh_integrator_pda(&authority);
        let (escrow, _) = escrow_pda(&buyer, DEAL);
        let (vault, _) = escrow_vault_pda(&escrow);
        let (bond_vault, _) = bond_vault_pda(&escrow);

        let mut accounts = vec![
            (addr(&buyer), wallet(10_000_000_000)),
            (addr(&seller), wallet(10_000_000_000)),
            (addr(&mint), spl_mint(DECIMALS as u8)),
            (addr(&protocol_mint), spl_mint(DECIMALS as u8)),
            (
                addr(&integrator),
                mesh_integrator_account(&authority, &ESCROW_PROGRAM),
            ),
            (
                addr(&mesh_config_pda().0),
                mesh_config_account(&protocol_mint),
            ),
            (addr(&escrow), missing()),
            (addr(&vault), missing()),
            (addr(&bond_vault), missing()),
            (addr(&buyer_tokens), token_account(&mint, &buyer, total())),
            (addr(&seller_tokens), token_account(&mint, &seller, 0)),
            (
                addr(&buyer_bond_tokens),
                token_account(&settlement_mint, &buyer, usdc(FUNDED)),
            ),
            (
                addr(&seller_bond_tokens),
                token_account(&settlement_mint, &seller, usdc(FUNDED)),
            ),
            keyed_account_for_token_program(),
            keyed_account_for_system_program(),
        ];

        if offered.is_some() {
            accounts.push((addr(&settlement_mint), spl_mint(DECIMALS as u8)));
        }

        Self {
            buyer,
            seller,
            mint,
            settlement_mint,
            integrator,
            escrow,
            vault,
            bond_vault,
            buyer_tokens,
            seller_tokens,
            buyer_bond_tokens,
            seller_bond_tokens,
            accounts,
        }
    }

    fn create_ix(&self) -> Instruction {
        anchor_ix(
            &ESCROW_PROGRAM,
            reference_escrow::accounts::CreateEscrow {
                buyer: self.buyer,
                seller: self.seller,
                mint: self.mint,
                integrator: self.integrator,
                config: mesh_config_pda().0,
                settlement_mint: self.settlement_mint,
                escrow: self.escrow,
                buyer_tokens: self.buyer_tokens,
                vault: self.vault,
                buyer_bond_tokens: self.buyer_bond_tokens,
                seller_bond_tokens: self.seller_bond_tokens,
                bond_vault: self.bond_vault,
                token_program: TOKEN_PROGRAM,
                settlement_token_program: TOKEN_PROGRAM,
                system_program: SYSTEM_PROGRAM,
            },
            reference_escrow::instruction::CreateEscrow {
                deal_id: DEAL,
                milestones: milestones(),
            },
        )
    }

    fn create(&self) -> InstructionResult {
        mollusk().process_instruction(&self.create_ix(), &self.accounts)
    }

    /// Угода, яку вже уклали — разом із заставою в касі.
    fn created(&self) -> Vec<(Address, Account)> {
        let result = self.create();
        assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

        let mut accounts = self.accounts.clone();
        for key in [
            &self.escrow,
            &self.vault,
            &self.bond_vault,
            &self.buyer_tokens,
            &self.buyer_bond_tokens,
            &self.seller_bond_tokens,
        ] {
            replace(&mut accounts, key, resulting(&result, key).clone());
        }
        accounts
    }

    fn release_ix(&self, milestone: u8) -> Instruction {
        anchor_ix(
            &ESCROW_PROGRAM,
            reference_escrow::accounts::ReleaseMilestone {
                buyer: self.buyer,
                escrow: self.escrow,
                mint: self.mint,
                seller_tokens: self.seller_tokens,
                vault: self.vault,
                settlement_mint: self.settlement_mint,
                buyer_bond_tokens: self.buyer_bond_tokens,
                seller_bond_tokens: self.seller_bond_tokens,
                bond_vault: self.bond_vault,
                token_program: TOKEN_PROGRAM,
                settlement_token_program: TOKEN_PROGRAM,
            },
            reference_escrow::instruction::ReleaseMilestone { milestone },
        )
    }

    fn dispute(&self) -> Pubkey {
        mesh_dispute_pda(&self.integrator, 0).0
    }

    /// Угода, у якій віха `DISPUTED` уже під розглядом, і сам розгляд із
    /// заданим вердиктом. Спір викладається фікстурою: виконання вердикту не
    /// залежить від того, як саме він був винесений.
    fn disputed(&self, by_seller: bool, verdict: Verdict, deposit: u64) -> Vec<(Address, Account)> {
        let mut accounts = self.created();
        let dispute = self.dispute();

        let (claimant, respondent) = if by_seller {
            (self.seller, self.buyer)
        } else {
            (self.buyer, self.seller)
        };

        let mut hearing = mesh_dispute_state(
            &self.integrator,
            0,
            &self.escrow,
            &claimant,
            &respondent,
            milestones()[DISPUTED as usize],
        );
        hearing.state = DisputeState::Tallied;
        hearing.verdict = Some(verdict);
        hearing.policy.deposit = deposit;

        let mut escrow: Escrow = decode(resulting_account(&accounts, &self.escrow));
        escrow.milestones[DISPUTED as usize].state = MilestoneState::Disputed { dispute };
        let mut account = program_account(&ESCROW_PROGRAM, &escrow);
        account
            .data
            .resize(Escrow::space(escrow.milestones.len()), 0);

        replace(&mut accounts, &self.escrow, account);
        accounts.push((addr(&dispute), mesh_dispute_account(&hearing)));
        accounts
    }

    fn settle_ix(&self) -> Instruction {
        anchor_ix(
            &ESCROW_PROGRAM,
            reference_escrow::accounts::SettleMilestone {
                escrow: self.escrow,
                dispute: self.dispute(),
                mint: self.mint,
                buyer_tokens: self.buyer_tokens,
                seller_tokens: self.seller_tokens,
                vault: self.vault,
                settlement_mint: self.settlement_mint,
                buyer_bond_tokens: self.buyer_bond_tokens,
                seller_bond_tokens: self.seller_bond_tokens,
                bond_vault: self.bond_vault,
                token_program: TOKEN_PROGRAM,
                settlement_token_program: TOKEN_PROGRAM,
            },
            reference_escrow::instruction::SettleMilestone {
                milestone: DISPUTED,
            },
        )
    }

    /// Розгляд, який дійшов до виплати. `deposit` — те, що ініціатор справді
    /// вніс, тобто знімок політики в самому спорі.
    fn settled(&self, by_seller: bool, verdict: Verdict, deposit: u64) -> InstructionResult {
        let accounts = self.disputed(by_seller, verdict, deposit);
        let result = mollusk_at(APPEAL_DEADLINE).process_instruction(&self.settle_ix(), &accounts);
        assert!(result.program_result.is_ok(), "{:?}", result.raw_result);
        result
    }

    fn balance(&self, result: &InstructionResult, key: &Pubkey) -> u64 {
        token_state(resulting(result, key)).amount
    }
}

/// Акаунт із набору, а не з результату — фікстурі потрібен стан **до** виклику.
fn resulting_account<'a>(accounts: &'a [(Address, Account)], key: &Pubkey) -> &'a Account {
    let key = addr(key);
    accounts
        .iter()
        .find(|(candidate, _)| *candidate == key)
        .map(|(_, account)| account)
        .unwrap_or_else(|| panic!("{key} is not among the accounts"))
}

// ── укладання ───────────────────────────────────────────────────────────────

/// Застава замикається з **обох** боків і по одній на кожну віху. Одна застава
/// на всю угоду означала б гонку: спори над різними віхами йдуть одночасно, і
/// перший, що дійшов до виплати, забрав би джерело в усіх інших.
#[test]
fn locks_a_bond_from_both_sides_for_every_milestone() {
    let fixture = Fixture::new();
    let result = fixture.create();
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    assert_eq!(
        fixture.balance(&result, &fixture.bond_vault),
        2 * bonds_per_side()
    );
    for tokens in [&fixture.buyer_bond_tokens, &fixture.seller_bond_tokens] {
        assert_eq!(
            fixture.balance(&result, tokens),
            usdc(FUNDED) - bonds_per_side()
        );
    }
}

/// Каса застав і каса угоди — різні акаунти з різними активами. Предмет угоди
/// дістається одній стороні, застава розходиться між обома, і змішати їх
/// означало б платити віху з чужої застави.
#[test]
fn keeps_the_bond_out_of_the_deal_vault() {
    let fixture = Fixture::new();
    let result = fixture.create();

    assert_eq!(fixture.balance(&result, &fixture.vault), total());

    let bonds = token_state(resulting(&result, &fixture.bond_vault));
    assert_eq!(bonds.owner, addr(&fixture.escrow));
    assert_eq!(bonds.mint, addr(&fixture.settlement_mint));
}

/// Розмір застави фіксується при укладанні. Політика інтегратора змінна, і
/// застава, що росла б разом із нею, вимагала б доносити кошти в угоду, яку вже
/// підписали.
#[test]
fn writes_the_bond_and_its_asset_into_the_deal() {
    let fixture = Fixture::new();
    let result = fixture.create();
    let escrow: Escrow = decode(resulting(&result, &fixture.escrow));

    assert_eq!(escrow.bond, bond());
    assert_eq!(escrow.settlement_mint, fixture.settlement_mint);
}

/// Заставою відшкодовується **депозит**, а він іде в розрахунковому активі
/// протоколу (`FR-011a`). Застава в чужому активі не відшкодувала б нічого — і
/// з'ясувалося б це в момент виплати, коли розгляд уже відбувся.
#[test]
fn refuses_a_bond_in_anything_but_the_settlement_asset() {
    let fixture = Fixture::with_settlement_mint(Some(Pubkey::new_unique()));
    let result = fixture.create();

    assert!(failed_with(&result, EscrowError::WrongSettlementMint));
}

#[test]
fn tells_the_terms_of_the_deal_in_one_event() {
    let (mollusk, logs) = mollusk_with_logs();
    let fixture = Fixture::new();
    let result = mollusk.process_instruction(&fixture.create_ix(), &fixture.accounts);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let events: Vec<EscrowOpened> = emitted(&logs);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].bond, bond());
    assert_eq!(events[0].total, total());
}

// ── віха, закрита без спору ─────────────────────────────────────────────────

/// Віха, закрита без спору, розгляду не коштувала нікому — застава
/// повертається обом. Інакше в касі лишились би замкненими справжні гроші, а не
/// оренда.
#[test]
fn gives_both_bonds_back_with_a_milestone_closed_without_a_dispute() {
    let fixture = Fixture::new();
    let accounts = fixture.created();

    let result = mollusk().process_instruction(&fixture.release_ix(0), &accounts);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    for tokens in [&fixture.buyer_bond_tokens, &fixture.seller_bond_tokens] {
        assert_eq!(
            fixture.balance(&result, tokens),
            usdc(FUNDED) - bonds_per_side() + bond()
        );
    }
    assert_eq!(
        fixture.balance(&result, &fixture.bond_vault),
        2 * bonds_per_side() - 2 * bond()
    );
}

/// Повертається застава **однієї** віхи, а не всі. Решта лишається за віхами,
/// які ще можуть стати спором.
#[test]
fn returns_only_the_bond_of_the_milestone_it_closed() {
    let fixture = Fixture::new();
    let mut accounts = fixture.created();

    for milestone in 0..milestones().len() as u8 {
        let result = mollusk().process_instruction(&fixture.release_ix(milestone), &accounts);
        assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

        for key in [
            &fixture.escrow,
            &fixture.vault,
            &fixture.bond_vault,
            &fixture.seller_tokens,
            &fixture.buyer_bond_tokens,
            &fixture.seller_bond_tokens,
        ] {
            replace(&mut accounts, key, resulting(&result, key).clone());
        }
    }

    // Угода дороблена: каса застав порожня, у кожного своє.
    assert_eq!(
        token_state(resulting_account(&accounts, &fixture.bond_vault)).amount,
        0
    );
    for tokens in [&fixture.buyer_bond_tokens, &fixture.seller_bond_tokens] {
        assert_eq!(
            token_state(resulting_account(&accounts, tokens)).amount,
            usdc(FUNDED)
        );
    }
}

// ── вердикт ─────────────────────────────────────────────────────────────────

/// Виконавець відкрив спір і виграв: розгляд оплатив замовник. Ініціатор
/// отримує назад свою заставу **і** депозит із застави програвшого — тобто за
/// розгляд не заплатив нічого, а програвший заплатив рівно один раз.
#[test]
fn makes_the_loser_pay_the_hearing_when_the_initiator_wins() {
    let fixture = Fixture::new();
    let result = fixture.settled(true, Verdict::Claimant, bond());

    assert_eq!(
        fixture.balance(&result, &fixture.seller_bond_tokens),
        usdc(FUNDED) - bonds_per_side() + 2 * bond()
    );
    assert_eq!(
        fixture.balance(&result, &fixture.buyer_bond_tokens),
        usdc(FUNDED) - bonds_per_side()
    );
}

/// Той самий вердикт із протилежним ініціатором рухає заставу в протилежний
/// бік: `Verdict::Claimant` — це «позиція ініціатора перемогла», а не «платіть
/// виконавцю».
#[test]
fn follows_the_role_of_the_initiator_not_the_side_of_the_deal() {
    let fixture = Fixture::new();
    let result = fixture.settled(false, Verdict::Claimant, bond());

    assert_eq!(
        fixture.balance(&result, &fixture.buyer_bond_tokens),
        usdc(FUNDED) - bonds_per_side() + 2 * bond()
    );
    assert_eq!(
        fixture.balance(&result, &fixture.seller_bond_tokens),
        usdc(FUNDED) - bonds_per_side()
    );
}

/// Програв сам ініціатор — обидві застави повертаються. Розгляд уже оплачений
/// його депозитом, і стягнути ще й заставу означало б покарати за ту саму
/// поразку двічі, подарувавши різницю відповідачу.
#[test]
fn charges_a_losing_initiator_once_and_only_with_his_deposit() {
    let fixture = Fixture::new();
    let result = fixture.settled(true, Verdict::Respondent, bond());

    for tokens in [&fixture.buyer_bond_tokens, &fixture.seller_bond_tokens] {
        assert_eq!(
            fixture.balance(&result, tokens),
            usdc(FUNDED) - bonds_per_side() + bond()
        );
    }
}

/// Статус-кво повертає віху в `Pending` — тобто вона й далі може стати спором,
/// і застава мусить лишитись за нею. Ніхто її не втрачає: вона повернеться
/// разом із віхою, коли ту нарешті закриють. Ціна невдалої ескалації лишається
/// одна — депозит ініціатора.
#[test]
fn keeps_the_bonds_while_the_milestone_is_still_open() {
    let fixture = Fixture::new();
    let result = fixture.settled(true, Verdict::StatusQuo, bond());

    assert_eq!(
        fixture.balance(&result, &fixture.bond_vault),
        2 * bonds_per_side()
    );
    for tokens in [&fixture.buyer_bond_tokens, &fixture.seller_bond_tokens] {
        assert_eq!(
            fixture.balance(&result, tokens),
            usdc(FUNDED) - bonds_per_side()
        );
    }
    assert_eq!(
        decode::<Escrow>(resulting(&result, &fixture.escrow)).milestones[DISPUTED as usize].state,
        MilestoneState::Pending
    );
}

/// `FR-026f`: політика подорожчала після укладання угоди, і застави на повне
/// відшкодування не вистачає. Відшкодовується скільки замкнено, різниця не
/// стягується ні з кого — застава відповідача не йде в мінус.
#[test]
fn reimburses_what_is_locked_when_the_policy_got_more_expensive() {
    let fixture = Fixture::new();
    let result = fixture.settled(true, Verdict::Claimant, 3 * bond());

    assert_eq!(
        fixture.balance(&result, &fixture.seller_bond_tokens),
        usdc(FUNDED) - bonds_per_side() + 2 * bond()
    );
    assert_eq!(
        fixture.balance(&result, &fixture.buyer_bond_tokens),
        usdc(FUNDED) - bonds_per_side()
    );
}

/// Політика подешевшала: відшкодовується депозит, а не застава. Решта — кошти
/// відповідача, і забирати їх немає підстав.
#[test]
fn reimburses_only_the_deposit_when_the_policy_got_cheaper() {
    let fixture = Fixture::new();
    let cheaper = bond() / 5;
    let result = fixture.settled(true, Verdict::Claimant, cheaper);

    assert_eq!(
        fixture.balance(&result, &fixture.seller_bond_tokens),
        usdc(FUNDED) - bonds_per_side() + bond() + cheaper
    );
    assert_eq!(
        fixture.balance(&result, &fixture.buyer_bond_tokens),
        usdc(FUNDED) - bonds_per_side() + bond() - cheaper
    );
}

/// Виплата за самим вердиктом застави не помічає — `FR-026f`. Предмет спору й
/// оплата розгляду живуть у різних касах і в різних активах.
#[test]
fn never_lets_the_bond_touch_the_milestone_itself() {
    let fixture = Fixture::new();
    let result = fixture.settled(true, Verdict::Claimant, 3 * bond());

    assert_eq!(
        fixture.balance(&result, &fixture.seller_tokens),
        milestones()[DISPUTED as usize]
    );
    assert_eq!(
        fixture.balance(&result, &fixture.vault),
        total() - milestones()[DISPUTED as usize]
    );
}

/// Хто зрештою поніс вартість розгляду — видно з однієї події (`FR-026a`), а не
/// з трьох переказів у двох програмах.
#[test]
fn says_in_the_event_how_much_the_loser_paid() {
    let cases = [
        (Verdict::Claimant, bond(), bond()),
        (Verdict::Respondent, bond(), 0),
        (Verdict::StatusQuo, bond(), 0),
        // Політика подорожчала: відшкодовано стільки, скільки замкнено.
        (Verdict::Claimant, 3 * bond(), bond()),
    ];

    for (verdict, deposit, expected) in cases {
        let (mollusk, logs) = mollusk_with_logs();
        let fixture = Fixture::new();
        let accounts = fixture.disputed(true, verdict, deposit);
        let result = {
            let mut mollusk = mollusk;
            mollusk.sysvars.clock.unix_timestamp = APPEAL_DEADLINE;
            mollusk.process_instruction(&fixture.settle_ix(), &accounts)
        };
        assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

        let events: Vec<MilestoneSettled> = emitted(&logs);
        assert_eq!(events.len(), 1, "{verdict:?}");
        assert_eq!(events[0].reimbursed, expected, "{verdict:?}");
    }
}

// ── чого застава не дозволяє ────────────────────────────────────────────────

/// Відшкодування не можна перенаправити. Обидва токен-акаунти прив'язані до
/// сторін угоди констрейнтами, тож дозвільний виклик не дає вибрати отримувача
/// — так само, як і у виплаті самої віхи.
#[test]
fn refuses_to_send_the_bond_to_an_account_of_a_stranger() {
    let fixture = Fixture::new();
    let stranger = Pubkey::new_unique();
    let stranger_tokens = Pubkey::new_unique();

    let mut accounts = fixture.disputed(true, Verdict::Claimant, bond());
    accounts.push((
        addr(&stranger_tokens),
        token_account(&fixture.settlement_mint, &stranger, 0),
    ));

    let ix = anchor_ix(
        &ESCROW_PROGRAM,
        reference_escrow::accounts::SettleMilestone {
            escrow: fixture.escrow,
            dispute: fixture.dispute(),
            mint: fixture.mint,
            buyer_tokens: fixture.buyer_tokens,
            seller_tokens: fixture.seller_tokens,
            vault: fixture.vault,
            settlement_mint: fixture.settlement_mint,
            buyer_bond_tokens: fixture.buyer_bond_tokens,
            seller_bond_tokens: stranger_tokens,
            bond_vault: fixture.bond_vault,
            token_program: TOKEN_PROGRAM,
            settlement_token_program: TOKEN_PROGRAM,
        },
        reference_escrow::instruction::SettleMilestone {
            milestone: DISPUTED,
        },
    );

    let result = mollusk_at(APPEAL_DEADLINE).process_instruction(&ix, &accounts);
    assert!(result.program_result.is_err());
}

/// Каса застав належить PDA угоди, а не стороні. Приватного ключа до неї не
/// існує, тож застава виходить лише тим шляхом, який програма підписала сама —
/// і жодна інструкція VerdictMesh сюди не дістає (`FR-014`).
#[test]
fn never_puts_the_bond_vault_under_a_key_anyone_holds() {
    let fixture = Fixture::new();
    let accounts = fixture.created();
    let vault = token_state(resulting_account(&accounts, &fixture.bond_vault));

    assert_eq!(vault.owner, addr(&fixture.escrow));
    assert_ne!(vault.owner, addr(&fixture.buyer));
    assert_ne!(vault.owner, addr(&fixture.seller));

    let dispute: Option<&(Address, Account)> = accounts
        .iter()
        .find(|(key, _)| *key == addr(&mesh_dispute_pda(&fixture.integrator, 0).0));
    assert!(dispute.is_none());
}
