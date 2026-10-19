/// Seeds live in one place because two parties derive them: the program itself and anyone
/// outside it — tests, the SDK, an integrator's escrow. A mismatch between two copies of a
/// string is not a compile error; it yields a PDA with nothing behind it.
pub const CONFIG: &[u8] = b"config";
pub const INTEGRATOR: &[u8] = b"integrator";
pub const REGISTRY: &[u8] = b"registry";
pub const JUROR: &[u8] = b"juror";
pub const JUROR_INDEX: &[u8] = b"juror_idx";
pub const DISPUTE: &[u8] = b"dispute";
pub const VOTE: &[u8] = b"vote";
pub const STAKE_VAULT: &[u8] = b"stake_vault";
pub const DISPUTE_VAULT: &[u8] = b"dispute_vault";
