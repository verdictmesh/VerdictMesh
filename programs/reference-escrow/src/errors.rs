use anchor_lang::prelude::*;

#[error_code]
pub enum EscrowError {
    #[msg("A deal needs two different parties")]
    InvalidParties,
    #[msg("Milestone amounts are empty, too many, or worth nothing")]
    InvalidMilestones,
    #[msg("There is no milestone with this number in the deal")]
    UnknownMilestone,
    #[msg("The milestone is not open: it is already settled or under dispute")]
    MilestoneNotPending,
    #[msg("Only the buyer or the seller of this deal may act on it")]
    NotAParty,
    #[msg("The dispute must be opened under the policy the deal was created with")]
    WrongIntegrator,
    #[msg("The integrator record points at another escrow program")]
    WrongArbitrationProgram,
    #[msg("Arithmetic overflow")]
    Overflow,
    #[msg("This dispute was opened over another escrow")]
    NotOurDispute,
    #[msg("The dispute has no verdict yet")]
    VerdictPending,
    #[msg("The verdict is frozen while the dispute is under appeal")]
    VerdictUnderAppeal,
    #[msg("The appeal window has not closed yet")]
    AppealWindowOpen,
    #[msg("This milestone is not under the dispute that was brought")]
    MilestoneNotUnderThisDispute,
    #[msg("The verdict names a party that is not in this deal")]
    VerdictNamesAStranger,
}
