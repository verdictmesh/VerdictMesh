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
}
