use anchor_lang::prelude::*;

#[error_code]
pub enum VerdictMeshError {
    #[msg("Policy parameters are inconsistent")]
    InvalidPolicy,
    #[msg("Juror registry has fewer jurors than the panel requires")]
    RegistryTooSmall,
    #[msg("Juror stake is below the amount required by the policy")]
    InsufficientStake,
    #[msg("Juror still participates in an unfinalized dispute")]
    JurorLocked,
    #[msg("Signer is not on the panel for this dispute")]
    NotOnPanel,
    #[msg("Action is not allowed in the current dispute state")]
    WrongState,
    #[msg("The window for this action has closed")]
    WindowClosed,
    #[msg("The window for this action has not opened yet")]
    WindowOpen,
    #[msg("Revealed vote does not match the submitted commitment")]
    CommitmentMismatch,
    #[msg("Report fingerprint may only be written by the reporter role")]
    NotReporter,
    #[msg("Report fingerprint is already set and cannot be changed")]
    ReportAlreadyAttested,
    #[msg("Dispute has already been escalated once")]
    AlreadyEscalated,
    #[msg("Dispute amount is above the optimistic threshold")]
    AboveOptimisticThreshold,
    #[msg("Arithmetic overflow")]
    Overflow,
    #[msg("Reporter role cannot be the default key")]
    InvalidReporter,
    #[msg("A dispute needs two different parties")]
    InvalidParties,
    #[msg("A dispute must be opened over a non-zero locked amount")]
    InvalidAmount,
    #[msg("Both parties must submit a statement fingerprint")]
    MissingClaim,
    #[msg("Registry tail accounts do not match the slot being vacated")]
    InvalidRegistryTail,
    #[msg("The entropy slot of this dispute is no longer in SlotHashes")]
    EntropyUnavailable,
    #[msg("The juror accounts do not enumerate the registry")]
    InvalidPanelAccounts,
    #[msg("The panel for this dispute has already been selected")]
    PanelAlreadySelected,
    #[msg("This vote has already been revealed")]
    AlreadyRevealed,
    #[msg("A commitment from an earlier round can no longer be revealed")]
    StaleCommitment,
    #[msg("The juror accounts do not enumerate the panel of this dispute")]
    InvalidSettlementAccounts,
    #[msg("Protocol treasury cannot be the default key")]
    InvalidTreasury,
    #[msg("The arbitration deposit is paid by the party opening the dispute")]
    NotTheDepositor,
}
