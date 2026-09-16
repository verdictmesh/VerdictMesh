//! T022 — milestone-ескроу: замикання коштів і відкриття спору (`FR-012`).
//!
//! Тут перевіряється не стільки ескроу, скільки **інтеграція**: спір відкриває
//! чужа програма справжнім CPI, підписуючись власним PDA, і VerdictMesh мусить
//! або прийняти цей підпис, або відмовити. Обидві програми в mollusk справжні —
//! заглушка перевіряла б лише те, що тест правильно уявляє собі чужі
//! констрейнти.
//!
//! **Найважливіше в цьому файлі — те, чого ескроу не робить.** Він не отримує
//! від VerdictMesh жодного повноваження і не віддає йому жодного: після
//! відкриття спору кошти угоди лишаються під владою PDA ескроу, а вердикт
//! звідси **витягнуть** окремою інструкцією (T023).

#[allow(dead_code)]
#[path = "harness.rs"]
mod harness;

use anchor_lang::solana_program::pubkey::Pubkey;
use harness::*;
use mollusk_svm::{program::keyed_account_for_system_program, result::InstructionResult};
use reference_escrow::{
    claims::{claim_of, Position},
    events::{EscrowOpened, MilestoneDisputed, MilestoneReleased},
    state::{Escrow, MilestoneState},
    EscrowError,
};
use solana_account::Account;
use solana_address::Address;
use solana_instruction::Instruction;
use verdict_mesh::state::Dispute;

const DEAL: u64 = 7;

fn milestones() -> Vec<u64> {
    vec![usdc(10), usdc(20), usdc(30)]
}

fn total() -> u64 {
    milestones().iter().sum()
}

struct Fixture {
    buyer: Pubkey,
    seller: Pubkey,
    authority: Pubkey,
    mint: Pubkey,
    settlement_mint: Pubkey,
    integrator: Pubkey,
    escrow: Pubkey,
    vault: Pubkey,
    buyer_tokens: Pubkey,
    seller_tokens: Pubkey,
    /// Токени сторін у розрахунковому активі протоколу, а не в активі угоди
    /// (`FR-011a`). З них іде і застава за розгляд при укладанні (`FR-026e`), і
    /// депозит при відкритті спору (`FR-026`).
    buyer_settlement: Pubkey,
    seller_settlement: Pubkey,
    bond_vault: Pubkey,
    accounts: Vec<(Address, Account)>,
}

impl Fixture {
    fn new() -> Self {
        let buyer = Pubkey::new_unique();
        let seller = Pubkey::new_unique();
        let authority = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        let settlement_mint = Pubkey::new_unique();
        let buyer_tokens = Pubkey::new_unique();
        let seller_tokens = Pubkey::new_unique();
        let buyer_settlement = Pubkey::new_unique();
        let seller_settlement = Pubkey::new_unique();

        let (integrator, _) = mesh_integrator_pda(&authority);
        let (escrow, _) = escrow_pda(&buyer, DEAL);
        let (vault, _) = escrow_vault_pda(&escrow);
        let (bond_vault, _) = bond_vault_pda(&escrow);
        let (dispute, _) = mesh_dispute_pda(&integrator, 0);

        let accounts = vec![
            (addr(&buyer), wallet(10_000_000_000)),
            (addr(&seller), wallet(10_000_000_000)),
            (addr(&mint), spl_mint(DECIMALS as u8)),
            (addr(&settlement_mint), spl_mint(DECIMALS as u8)),
            (
                addr(&integrator),
                mesh_integrator_account(&authority, &ESCROW_PROGRAM),
            ),
            (addr(&escrow), missing()),
            (addr(&vault), missing()),
            (addr(&bond_vault), missing()),
            (addr(&buyer_tokens), token_account(&mint, &buyer, usdc(500))),
            (addr(&seller_tokens), token_account(&mint, &seller, 0)),
            // VerdictMesh — потрібне лише спору, але фікстура одна на весь файл:
            // окремий набір під кожну інструкцію розійшовся б з цим на першій же
            // зміні.
            (
                addr(&mesh_config_pda().0),
                mesh_config_account(&settlement_mint),
            ),
            (
                addr(&buyer_settlement),
                token_account(&settlement_mint, &buyer, usdc(50)),
            ),
            (
                addr(&seller_settlement),
                token_account(&settlement_mint, &seller, usdc(50)),
            ),
            (addr(&dispute), missing()),
            (addr(&mesh_dispute_vault_pda(&dispute).0), missing()),
            keyed_account_for_token_program(),
            keyed_account_for_mesh_program(),
            keyed_account_for_system_program(),
        ];

        Self {
            buyer,
            seller,
            authority,
            mint,
            settlement_mint,
            integrator,
            escrow,
            vault,
            buyer_tokens,
            seller_tokens,
            buyer_settlement,
            seller_settlement,
            bond_vault,
            accounts,
        }
    }

    fn create_ix(&self, milestones: Vec<u64>) -> Instruction {
        self.create_ix_with(self.integrator, milestones)
    }

    fn create_ix_with(&self, integrator: Pubkey, milestones: Vec<u64>) -> Instruction {
        anchor_ix(
            &ESCROW_PROGRAM,
            reference_escrow::accounts::CreateEscrow {
                buyer: self.buyer,
                seller: self.seller,
                mint: self.mint,
                integrator,
                config: mesh_config_pda().0,
                settlement_mint: self.settlement_mint,
                escrow: self.escrow,
                buyer_tokens: self.buyer_tokens,
                vault: self.vault,
                buyer_bond_tokens: self.buyer_settlement,
                seller_bond_tokens: self.seller_settlement,
                bond_vault: self.bond_vault,
                token_program: TOKEN_PROGRAM,
                settlement_token_program: TOKEN_PROGRAM,
                system_program: SYSTEM_PROGRAM,
            },
            reference_escrow::instruction::CreateEscrow {
                deal_id: DEAL,
                milestones,
            },
        )
    }

    /// Угода, яку вже створили. Проганяти `create` перед кожним тестом
    /// звільнення чи спору означало б зав'язати кожен із них на іншу інструкцію.
    fn created(&self) -> Vec<(Address, Account)> {
        let result = mollusk().process_instruction(&self.create_ix(milestones()), &self.accounts);
        assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

        let mut accounts = self.accounts.clone();
        for key in [
            &self.escrow,
            &self.vault,
            &self.buyer_tokens,
            &self.bond_vault,
            &self.buyer_settlement,
            &self.seller_settlement,
        ] {
            replace(&mut accounts, key, resulting(&result, key).clone());
        }
        accounts
    }

    fn release_ix(&self, milestone: u8) -> Instruction {
        self.release_ix_by(self.buyer, milestone)
    }

    fn release_ix_by(&self, buyer: Pubkey, milestone: u8) -> Instruction {
        anchor_ix(
            &ESCROW_PROGRAM,
            reference_escrow::accounts::ReleaseMilestone {
                buyer,
                escrow: self.escrow,
                mint: self.mint,
                seller_tokens: self.seller_tokens,
                vault: self.vault,
                settlement_mint: self.settlement_mint,
                buyer_bond_tokens: self.buyer_settlement,
                seller_bond_tokens: self.seller_settlement,
                bond_vault: self.bond_vault,
                token_program: TOKEN_PROGRAM,
                settlement_token_program: TOKEN_PROGRAM,
            },
            reference_escrow::instruction::ReleaseMilestone { milestone },
        )
    }

    fn dispute(&self, dispute_id: u64) -> Pubkey {
        mesh_dispute_pda(&self.integrator, dispute_id).0
    }

    fn dispute_ix(&self, claimant: Pubkey, milestone: u8) -> Instruction {
        let tokens = if claimant == self.seller {
            self.seller_settlement
        } else {
            self.buyer_settlement
        };
        self.dispute_ix_full(claimant, tokens, self.integrator, milestone)
    }

    fn dispute_ix_full(
        &self,
        claimant: Pubkey,
        claimant_tokens: Pubkey,
        integrator: Pubkey,
        milestone: u8,
    ) -> Instruction {
        let dispute = self.dispute(0);

        anchor_ix(
            &ESCROW_PROGRAM,
            reference_escrow::accounts::DisputeMilestone {
                claimant,
                escrow: self.escrow,
                integrator,
                config: mesh_config_pda().0,
                settlement_mint: self.settlement_mint,
                dispute,
                claimant_tokens,
                dispute_vault: mesh_dispute_vault_pda(&dispute).0,
                verdict_mesh_program: MESH_PROGRAM,
                token_program: TOKEN_PROGRAM,
                system_program: SYSTEM_PROGRAM,
            },
            reference_escrow::instruction::DisputeMilestone { milestone },
        )
    }

    fn escrow_state(&self, result: &InstructionResult) -> Escrow {
        decode(resulting(result, &self.escrow))
    }
}

// ── замикання коштів ────────────────────────────────────────────────────────

/// Уся сума замикається наперед. Ескроу, у який доносять, не є ескроу:
/// виконавець дізнався б про порожню касу вже після роботи.
#[test]
fn locks_the_whole_deal_before_the_first_milestone() {
    let fixture = Fixture::new();
    let result = mollusk().process_instruction(&fixture.create_ix(milestones()), &fixture.accounts);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    assert_eq!(
        token_state(resulting(&result, &fixture.vault)).amount,
        total()
    );
    assert_eq!(
        token_state(resulting(&result, &fixture.buyer_tokens)).amount,
        usdc(500) - total()
    );
}

#[test]
fn writes_the_parties_the_policy_and_the_milestones() {
    let fixture = Fixture::new();
    let result = mollusk().process_instruction(&fixture.create_ix(milestones()), &fixture.accounts);
    let escrow = fixture.escrow_state(&result);

    assert_eq!(escrow.buyer, fixture.buyer);
    assert_eq!(escrow.seller, fixture.seller);
    assert_eq!(escrow.mint, fixture.mint);
    assert_eq!(escrow.integrator, fixture.integrator);
    assert_eq!(escrow.deal_id, DEAL);
    assert_eq!(escrow.bump, escrow_pda(&fixture.buyer, DEAL).1);

    assert_eq!(escrow.milestones.len(), 3);
    for (entry, amount) in escrow.milestones.iter().zip(milestones()) {
        assert_eq!(entry.amount, amount);
        assert_eq!(entry.state, MilestoneState::Pending);
    }
}

/// Касою розпоряджається PDA угоди, а не замовник. Інакше він забрав би гроші
/// рівно тоді, коли роботу вже зроблено, і арбітраж не мав би над чим виносити
/// вердикт.
#[test]
fn puts_the_deal_under_the_authority_of_the_escrow() {
    let fixture = Fixture::new();
    let result = mollusk().process_instruction(&fixture.create_ix(milestones()), &fixture.accounts);

    let vault = token_state(resulting(&result, &fixture.vault));
    assert_eq!(vault.owner, addr(&fixture.escrow));
    assert_eq!(vault.mint, addr(&fixture.mint));
}

#[test]
fn sizes_the_account_by_the_number_of_milestones() {
    let fixture = Fixture::new();
    let result = mollusk().process_instruction(&fixture.create_ix(milestones()), &fixture.accounts);

    assert_eq!(
        resulting(&result, &fixture.escrow).data.len(),
        Escrow::space(3)
    );
}

/// Виконавець підписує не через ввічливість: разом з угодою він приймає
/// **політику розгляду**, за якою його ж і судитимуть. Угода, підписана однією
/// стороною, зробила б відповідачем того, хто на цей арбітраж не погоджувався.
#[test]
fn requires_the_seller_to_sign_the_deal() {
    let fixture = Fixture::new();
    let metas = fixture.create_ix(milestones()).accounts;

    let seller = metas
        .iter()
        .find(|meta| meta.pubkey == addr(&fixture.seller))
        .expect("the seller must be among the instruction accounts");

    assert!(seller.is_signer);
}

#[test]
fn refuses_a_deal_where_both_sides_are_the_same_key() {
    let fixture = Fixture::new();

    let ix = anchor_ix(
        &ESCROW_PROGRAM,
        reference_escrow::accounts::CreateEscrow {
            buyer: fixture.buyer,
            seller: fixture.buyer,
            mint: fixture.mint,
            integrator: fixture.integrator,
            config: mesh_config_pda().0,
            settlement_mint: fixture.settlement_mint,
            escrow: fixture.escrow,
            buyer_tokens: fixture.buyer_tokens,
            vault: fixture.vault,
            buyer_bond_tokens: fixture.buyer_settlement,
            seller_bond_tokens: fixture.buyer_settlement,
            bond_vault: fixture.bond_vault,
            token_program: TOKEN_PROGRAM,
            settlement_token_program: TOKEN_PROGRAM,
            system_program: SYSTEM_PROGRAM,
        },
        reference_escrow::instruction::CreateEscrow {
            deal_id: DEAL,
            milestones: milestones(),
        },
    );

    let result = mollusk().process_instruction(&ix, &fixture.accounts);
    assert!(failed_with(&result, EscrowError::InvalidParties));
}

/// Угода без віх нічого не замикає, а віха на нуль — крок, який неможливо ані
/// виплатити, ані оспорити з користю.
#[test]
fn refuses_a_deal_that_locks_nothing() {
    let fixture = Fixture::new();

    for milestones in [vec![], vec![usdc(10), 0]] {
        let result =
            mollusk().process_instruction(&fixture.create_ix(milestones), &fixture.accounts);
        assert!(failed_with(&result, EscrowError::InvalidMilestones));
    }
}

/// Стеля існує, щоб інструкції завжди вистачало обчислювального бюджету, а не
/// щоб обмежити угоду. Виявитись це мусить тут, а не на живих коштах.
#[test]
fn refuses_more_milestones_than_the_ceiling() {
    let fixture = Fixture::new();
    let too_many = vec![usdc(1); reference_escrow::state::MAX_MILESTONES as usize + 1];

    let result = mollusk().process_instruction(&fixture.create_ix(too_many), &fixture.accounts);
    assert!(failed_with(&result, EscrowError::InvalidMilestones));
}

/// Політика перевіряється при створенні, а не в момент спору. `Integrator`, що
/// вказує на іншу програму ескроу, не зміг би відкрити спір із цієї — і
/// з'ясувалося б це в найгіршу мить: гроші замкнені, а звернутись нікуди.
#[test]
fn refuses_a_policy_that_points_at_another_escrow_program() {
    let fixture = Fixture::new();
    let stranger = Pubkey::new_unique();
    let (integrator, _) = mesh_integrator_pda(&stranger);

    let mut accounts = fixture.accounts.clone();
    accounts.push((
        addr(&integrator),
        mesh_integrator_account(&stranger, &Pubkey::new_unique()),
    ));

    let result =
        mollusk().process_instruction(&fixture.create_ix_with(integrator, milestones()), &accounts);
    assert!(failed_with(&result, EscrowError::WrongArbitrationProgram));
}

/// Акаунт політики належить VerdictMesh, і Anchor звіряє власника. Підсунути
/// сюди підроблений `Integrator` нічим — інакше сторони погодились би на
/// правила, яких не існує.
#[test]
fn refuses_a_policy_account_owned_by_somebody_else() {
    let fixture = Fixture::new();

    let mut accounts = fixture.accounts.clone();
    let mut forged = mesh_integrator_account(&fixture.authority, &ESCROW_PROGRAM);
    forged.owner = addr(&ESCROW_PROGRAM);
    replace(&mut accounts, &fixture.integrator, forged);

    let result = mollusk().process_instruction(&fixture.create_ix(milestones()), &accounts);
    assert!(result.program_result.is_err());
}

#[test]
fn refuses_a_second_deal_with_the_same_number() {
    let fixture = Fixture::new();
    let accounts = fixture.created();

    let result = mollusk().process_instruction(&fixture.create_ix(milestones()), &accounts);
    assert!(result.program_result.is_err());
}

#[test]
fn emits_the_opening_event() {
    let fixture = Fixture::new();
    let (mollusk, logs) = mollusk_with_logs();

    let result = mollusk.process_instruction(&fixture.create_ix(milestones()), &fixture.accounts);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let events: Vec<EscrowOpened> = emitted(&logs);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].escrow, fixture.escrow);
    assert_eq!(events[0].buyer, fixture.buyer);
    assert_eq!(events[0].seller, fixture.seller);
    assert_eq!(events[0].integrator, fixture.integrator);
    assert_eq!(events[0].total, total());
    assert_eq!(events[0].milestones, 3);
}

// ── щасливий шлях ───────────────────────────────────────────────────────────

#[test]
fn pays_a_released_milestone_to_the_seller() {
    let fixture = Fixture::new();
    let accounts = fixture.created();

    let result = mollusk().process_instruction(&fixture.release_ix(1), &accounts);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    assert_eq!(
        token_state(resulting(&result, &fixture.seller_tokens)).amount,
        usdc(20)
    );
    assert_eq!(
        token_state(resulting(&result, &fixture.vault)).amount,
        total() - usdc(20)
    );

    let escrow = fixture.escrow_state(&result);
    assert_eq!(escrow.milestones[1].state, MilestoneState::Released);
    assert_eq!(escrow.milestones[0].state, MilestoneState::Pending);
    assert_eq!(escrow.milestones[2].state, MilestoneState::Pending);
}

/// Закриває віху лише замовник, бо лише він може віддати свої гроші
/// добровільно. Дозвільний виклик перетворив би ескроу на кран.
#[test]
fn refuses_a_release_by_anyone_but_the_buyer() {
    let fixture = Fixture::new();
    let accounts = fixture.created();

    let result =
        mollusk().process_instruction(&fixture.release_ix_by(fixture.seller, 1), &accounts);
    assert!(failed_with(&result, EscrowError::NotAParty));
}

#[test]
fn refuses_to_release_the_same_milestone_twice() {
    let fixture = Fixture::new();
    let mut accounts = fixture.created();

    let first = mollusk().process_instruction(&fixture.release_ix(0), &accounts);
    assert!(first.program_result.is_ok(), "{:?}", first.raw_result);

    for key in [&fixture.escrow, &fixture.vault, &fixture.seller_tokens] {
        replace(&mut accounts, key, resulting(&first, key).clone());
    }

    let second = mollusk().process_instruction(&fixture.release_ix(0), &accounts);
    assert!(failed_with(&second, EscrowError::MilestoneNotPending));
}

/// Номер поза межами — це не паніка в програмі, а зрозуміла відмова. Індексація
/// вектора без перевірки завершила б інструкцію без жодного пояснення.
#[test]
fn refuses_a_milestone_that_does_not_exist() {
    let fixture = Fixture::new();
    let accounts = fixture.created();

    let result = mollusk().process_instruction(&fixture.release_ix(3), &accounts);
    assert!(failed_with(&result, EscrowError::UnknownMilestone));
}

#[test]
fn emits_the_release_event() {
    let fixture = Fixture::new();
    let accounts = fixture.created();
    let (mollusk, logs) = mollusk_with_logs();

    let result = mollusk.process_instruction(&fixture.release_ix(2), &accounts);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let events: Vec<MilestoneReleased> = emitted(&logs);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].escrow, fixture.escrow);
    assert_eq!(events[0].milestone, 2);
    assert_eq!(events[0].amount, usdc(30));
}

// ── FR-012: відкриття спору чужою програмою ─────────────────────────────────

/// Головний тест файлу. Спір відкриває **ескроу**, підписуючись власним PDA, і
/// VerdictMesh цей підпис приймає. Усе, що нижче, — наслідки цього виклику.
#[test]
fn opens_a_dispute_over_the_milestone_through_verdict_mesh() {
    let fixture = Fixture::new();
    let accounts = fixture.created();

    let result = mollusk().process_instruction(&fixture.dispute_ix(fixture.seller, 1), &accounts);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let dispute: Dispute = decode(resulting(&result, &fixture.dispute(0)));
    assert_eq!(dispute.escrow_ref, fixture.escrow);
    assert_eq!(dispute.claimant, fixture.seller);
    assert_eq!(dispute.respondent, fixture.buyer);
    // Спір іде над **віхою**, а не над угодою: незгода щодо одного кроку не
    // ставить під сумнів усе, що вже прийнято.
    assert_eq!(dispute.amount, usdc(20));
}

/// Адреса розгляду лишається в стані віхи. Без неї виконання вердикту (T023)
/// прив'язувалось би до віхи за здогадкою, і будь-який інший спір цієї ж угоди
/// зійшовся б за `escrow_ref`.
#[test]
fn records_the_dispute_in_the_milestone_it_belongs_to() {
    let fixture = Fixture::new();
    let accounts = fixture.created();

    let result = mollusk().process_instruction(&fixture.dispute_ix(fixture.seller, 1), &accounts);
    let escrow = fixture.escrow_state(&result);

    assert_eq!(
        escrow.milestones[1].state,
        MilestoneState::Disputed {
            dispute: fixture.dispute(0)
        }
    );
    assert_eq!(escrow.milestones[0].state, MilestoneState::Pending);
    assert_eq!(escrow.milestones[2].state, MilestoneState::Pending);
}

/// Позиції виводяться з ролей, і жодна сторона не формулює твердження іншої.
/// Тест рахує обидва відбитки тією самою формулою, якою їх звірятиме офчейн.
#[test]
fn takes_each_side_position_from_its_role() {
    let fixture = Fixture::new();
    let accounts = fixture.created();

    let release = claim_of(&fixture.escrow, 1, Position::Release);
    let refund = claim_of(&fixture.escrow, 1, Position::Refund);

    let opened_by_seller =
        mollusk().process_instruction(&fixture.dispute_ix(fixture.seller, 1), &accounts);
    let dispute: Dispute = decode(resulting(&opened_by_seller, &fixture.dispute(0)));
    assert_eq!(dispute.claimant_claim_hash, release);
    assert_eq!(dispute.respondent_claim_hash, refund);

    let opened_by_buyer =
        mollusk().process_instruction(&fixture.dispute_ix(fixture.buyer, 1), &accounts);
    let dispute: Dispute = decode(resulting(&opened_by_buyer, &fixture.dispute(0)));
    assert_eq!(dispute.claimant, fixture.buyer);
    assert_eq!(dispute.respondent, fixture.seller);
    assert_eq!(dispute.claimant_claim_hash, refund);
    assert_eq!(dispute.respondent_claim_hash, release);
}

/// Депозит за розгляд платить ініціатор (`FR-026`), і в **розрахунковому**
/// активі протоколу, а не в активі угоди. Каса угоди при цьому не зрушила: спір
/// не дає VerdictMesh жодної влади над цими коштами. Застава, замкнена при
/// укладанні, теж лишається на місці — вона розійдеться разом із самою віхою.
#[test]
fn charges_the_initiator_the_review_deposit_and_leaves_the_deal_alone() {
    let fixture = Fixture::new();
    let accounts = fixture.created();

    let result = mollusk().process_instruction(&fixture.dispute_ix(fixture.seller, 1), &accounts);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let deposit = demo_policy().deposit;
    let bonds = deposit * milestones().len() as u64;
    assert_eq!(
        token_state(resulting(&result, &fixture.seller_settlement)).amount,
        usdc(50) - bonds - deposit
    );
    assert_eq!(
        token_state(resulting(
            &result,
            &mesh_dispute_vault_pda(&fixture.dispute(0)).0
        ))
        .amount,
        deposit
    );
    assert_eq!(
        token_state(resulting(&result, &fixture.vault)).amount,
        total()
    );
}

/// Сторонній не має чого вимагати від чужої угоди. Без цієї перевірки будь-хто
/// міг би замкнути чужу віху на час розгляду — за свій депозит, але за чужий
/// рахунок.
#[test]
fn refuses_a_dispute_from_someone_who_is_not_a_party() {
    let fixture = Fixture::new();
    let stranger = Pubkey::new_unique();
    let stranger_tokens = Pubkey::new_unique();

    let mut accounts = fixture.created();
    accounts.push((addr(&stranger), wallet(10_000_000_000)));
    accounts.push((
        addr(&stranger_tokens),
        token_account(&fixture.settlement_mint, &stranger, usdc(50)),
    ));

    let result = mollusk().process_instruction(
        &fixture.dispute_ix_full(stranger, stranger_tokens, fixture.integrator, 1),
        &accounts,
    );
    assert!(failed_with(&result, EscrowError::NotAParty));
}

/// Спір під чужою політикою — це спір за іншими вікнами, іншим кворумом і
/// іншим слешингом, ніж ті, на які сторони погодились. Єдина перевірка, яку
/// інтегратор зобов'язаний зробити сам.
#[test]
fn refuses_a_dispute_under_a_policy_the_deal_never_agreed_to() {
    let fixture = Fixture::new();
    let stranger = Pubkey::new_unique();
    let (other_integrator, _) = mesh_integrator_pda(&stranger);

    let mut accounts = fixture.created();
    accounts.push((
        addr(&other_integrator),
        mesh_integrator_account(&stranger, &ESCROW_PROGRAM),
    ));

    let result = mollusk().process_instruction(
        &fixture.dispute_ix_full(
            fixture.seller,
            fixture.seller_settlement,
            other_integrator,
            1,
        ),
        &accounts,
    );
    assert!(failed_with(&result, EscrowError::WrongIntegrator));
}

#[test]
fn refuses_a_dispute_over_a_milestone_that_is_already_settled() {
    let fixture = Fixture::new();
    let mut accounts = fixture.created();

    let released = mollusk().process_instruction(&fixture.release_ix(1), &accounts);
    assert!(released.program_result.is_ok(), "{:?}", released.raw_result);
    for key in [&fixture.escrow, &fixture.vault, &fixture.seller_tokens] {
        replace(&mut accounts, key, resulting(&released, key).clone());
    }

    let result = mollusk().process_instruction(&fixture.dispute_ix(fixture.seller, 1), &accounts);
    assert!(failed_with(&result, EscrowError::MilestoneNotPending));
}

/// Другий спір над тією ж віхою — це другий депозит за той самий предмет і два
/// вердикти, з яких виконати можна лише один.
#[test]
fn refuses_a_second_dispute_over_the_same_milestone() {
    let fixture = Fixture::new();
    let mut accounts = fixture.created();

    let first = mollusk().process_instruction(&fixture.dispute_ix(fixture.seller, 1), &accounts);
    assert!(first.program_result.is_ok(), "{:?}", first.raw_result);
    for key in [
        &fixture.escrow,
        &fixture.integrator,
        &fixture.seller_settlement,
    ] {
        replace(&mut accounts, key, resulting(&first, key).clone());
    }

    let second = mollusk().process_instruction(&fixture.dispute_ix(fixture.buyer, 1), &accounts);
    assert!(failed_with(&second, EscrowError::MilestoneNotPending));
}

#[test]
fn refuses_a_dispute_over_a_milestone_that_does_not_exist() {
    let fixture = Fixture::new();
    let accounts = fixture.created();

    let result = mollusk().process_instruction(&fixture.dispute_ix(fixture.seller, 9), &accounts);
    assert!(failed_with(&result, EscrowError::UnknownMilestone));
}

#[test]
fn emits_the_dispute_event() {
    let fixture = Fixture::new();
    let accounts = fixture.created();
    let (mollusk, logs) = mollusk_with_logs();

    let result = mollusk.process_instruction(&fixture.dispute_ix(fixture.buyer, 0), &accounts);
    assert!(result.program_result.is_ok(), "{:?}", result.raw_result);

    let events: Vec<MilestoneDisputed> = emitted(&logs);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].escrow, fixture.escrow);
    assert_eq!(events[0].milestone, 0);
    assert_eq!(events[0].dispute, fixture.dispute(0));
    assert_eq!(events[0].claimant, fixture.buyer);
    assert_eq!(events[0].amount, usdc(10));
}

// ── межа з VerdictMesh ──────────────────────────────────────────────────────

/// `FR-014` з боку інтегратора: спір не дає VerdictMesh жодного повноваження
/// над цією угодою. Доводиться це відсутністю — серед акаунтів інструкції немає
/// ані каси угоди, ані її мінта, тож VerdictMesh не бачить коштів, над якими
/// судить.
#[test]
fn never_shows_the_deal_vault_to_the_arbitration_program() {
    let fixture = Fixture::new();
    let metas = fixture.dispute_ix(fixture.seller, 1).accounts;

    for hidden in [&fixture.vault, &fixture.mint, &fixture.seller_tokens] {
        assert!(
            !metas.iter().any(|meta| meta.pubkey == addr(hidden)),
            "the arbitration call must not carry the funds it judges"
        );
    }
}

/// Ескроу не повторює чужі констрейнти: акаунти VerdictMesh перевіряє той, кому
/// вони належать. Тест доводить, що межа справді там — підмінений `config`
/// відхиляє **VerdictMesh**, а не ескроу.
#[test]
fn leaves_the_arbitration_accounts_to_the_arbitration_program() {
    let fixture = Fixture::new();
    let impostor = Pubkey::new_unique();

    let mut accounts = fixture.created();
    accounts.push((addr(&impostor), missing()));

    let mut ix = fixture.dispute_ix(fixture.seller, 1);
    for meta in &mut ix.accounts {
        if meta.pubkey == addr(&mesh_config_pda().0) {
            meta.pubkey = addr(&impostor);
        }
    }

    let result = mollusk().process_instruction(&ix, &accounts);
    assert!(result.program_result.is_err());
}
