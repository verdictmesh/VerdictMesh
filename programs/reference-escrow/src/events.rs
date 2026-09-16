use anchor_lang::prelude::*;

/// Події тут — не дзеркало VerdictMesh, а те, з чого fact-finding (T028)
/// відновлює **предмет** спору: угоду, її віхи й те, що з ними вже сталося.
/// Ончейн-факт із посиланням на підпис транзакції (`FR-016`) береться саме
/// звідси.
#[event]
pub struct EscrowOpened {
    pub escrow: Pubkey,
    pub buyer: Pubkey,
    pub seller: Pubkey,
    pub mint: Pubkey,
    pub integrator: Pubkey,
    pub total: u64,
    pub milestones: u8,
    /// Застава за розгляд однієї віхи з одного боку — `FR-026e`. Умови угоди
    /// видно з однієї події цілком: замкнено `total` предмета плюс
    /// `bond × milestones` з кожного боку.
    pub bond: u64,
}

#[event]
pub struct MilestoneReleased {
    pub escrow: Pubkey,
    pub milestone: u8,
    pub amount: u64,
}

/// Віха пішла на розгляд. `claimant` тут — не зайве дублювання спору: саме за
/// ним виконання вердикту (T023) розуміє, кому дістається `Verdict::Claimant`,
/// і спостерігач мусить бачити той самий зв'язок.
#[event]
pub struct MilestoneDisputed {
    pub escrow: Pubkey,
    pub milestone: u8,
    pub dispute: Pubkey,
    pub claimant: Pubkey,
    pub amount: u64,
}

/// Вердикт виконано — `FR-012`. `winner` порожній рівно за статус-кво: розгляд
/// закінчився, кошти не рухались, віха повернулась туди, звідки її взяли.
/// Самого вердикту тут немає навмисно — його вже оголосив VerdictMesh
/// (`DisputeTallied`), і другий запис того самого факту рано чи пізно
/// розійшовся б з першим.
#[event]
pub struct MilestoneSettled {
    pub escrow: Pubkey,
    pub milestone: u8,
    pub dispute: Pubkey,
    pub winner: Option<Pubkey>,
    pub amount: u64,
    /// Скільки застави програвшої сторони пішло на відшкодування депозиту
    /// ініціаторові — `FR-026a`. Нуль означає, що розгляд оплатив сам ініціатор:
    /// або він програв, або вердикт лишив сторони там, де вони були. Без цього
    /// поля відповідь на «хто зрештою поніс вартість розгляду» довелось би
    /// збирати з двох програм і трьох переказів.
    pub reimbursed: u64,
}
