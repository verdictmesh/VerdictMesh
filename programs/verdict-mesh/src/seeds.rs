/// Seed-и живуть в одному місці, бо їх виводять двоє: сама програма і будь-хто
/// зовні — тести, SDK, ескроу інтегратора. Розбіжність між двома копіями рядка
/// не дає помилки компіляції, вона дає PDA, за якою нічого немає.
pub const CONFIG: &[u8] = b"config";
pub const INTEGRATOR: &[u8] = b"integrator";
pub const REGISTRY: &[u8] = b"registry";
pub const JUROR: &[u8] = b"juror";
pub const JUROR_INDEX: &[u8] = b"juror_idx";
pub const DISPUTE: &[u8] = b"dispute";
pub const VOTE: &[u8] = b"vote";
pub const STAKE_VAULT: &[u8] = b"stake_vault";
pub const DISPUTE_VAULT: &[u8] = b"dispute_vault";
