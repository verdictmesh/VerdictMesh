use anchor_lang::prelude::*;

// Order is part of the deployed interface: a variant inserted in the middle
// shifts every code after it. Append only.
#[error_code]
pub enum DepinError {
    #[msg("An epoch needs two different parties")]
    InvalidParties,
    #[msg("The epoch must lock a reward")]
    InvalidReward,
    #[msg("The challenge window must be positive")]
    InvalidWindow,
    #[msg("The integrator record points at another escrow program")]
    WrongArbitrationProgram,
    #[msg("The reward must be held in the settlement asset of the protocol")]
    WrongRewardMint,
    #[msg("A share is at most 10000 basis points")]
    InvalidShare,
    #[msg("Only the network of this epoch may do this")]
    NotTheNetwork,
    #[msg("Only the operator of this epoch may do this")]
    NotTheOperator,
    #[msg("Usage for this epoch has already been reported")]
    AlreadyReported,
    #[msg("The epoch has no open report")]
    NotReported,
    #[msg("The challenge window has closed")]
    ChallengeWindowClosed,
    #[msg("The challenge window is still open")]
    ChallengeWindowOpen,
    #[msg("A challenge must claim more than the report gives")]
    ClaimNotAboveReport,
    #[msg("The two offers differ by less than one token unit")]
    NothingContested,
    #[msg("The dispute must be opened under the policy the epoch was funded with")]
    WrongIntegrator,
    #[msg("This epoch is not under the dispute that was brought")]
    NotUnderThisDispute,
    #[msg("The verdict names a party that is not in this epoch")]
    VerdictNamesAStranger,
    #[msg("Arithmetic overflow")]
    Overflow,
}
