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
