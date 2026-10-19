//! Juror panel selection — `FR-006`, and the boundary `FR-006a` requires.
//!
//! **What exactly is isolated here.** `FR-006a` requires that replacing the entropy
//! source with an external verifiable generator must not touch the juror registry,
//! voting, or verdict execution. That is why this module holds **both** halves: where
//! the randomness comes from (`entropy_of`) and how the panel is derived from it
//! (`draw`). Swapping the slot hash for a VRF means rewriting the first without
//! touching the second — and nothing outside this file.
//!
//! **The weak spot, stated openly.** A slot hash can be influenced by that
//! slot's leader, so the panel is in theory predictable to whoever produces
//! the slot. This is recorded in `SPEC.md` → "Out of scope" and in `PLAN.md` →
//! risks; the boundary exists precisely so that a VRF can go here on mainnet.
//! For the devnet demo the risk is accepted deliberately.
//!
//! **Why entropy is tied to the opening slot rather than the selection slot.**
//! Otherwise the selection could be re-rolled: retrying slot after slot until
//! the panel is to one's liking. Binding to `Dispute.entropy_slot` fixes the
//! panel at the moment the dispute opens and keeps it unchanged afterwards.

use std::cmp::Ordering;

use anchor_lang::prelude::*;
use solana_keccak_hasher::hashv;

use crate::{errors::VerdictMeshError, state::MAX_PANEL_SIZE};

/// One entry of the `SlotHashes` sysvar: slot (`u64` LE) and hash (32 bytes).
const SLOT_HASH_ENTRY: usize = 40;

/// Address of the `SlotHashes` sysvar. Declared here rather than taken from
/// Anchor's re-export: in 0.32.1 the whole `solana_program::sysvar` module is
/// marked deprecated, and `clippy -D warnings` does not let that pass.
pub const SLOT_HASHES_ID: Pubkey =
    anchor_lang::solana_program::pubkey::pubkey!("SysvarS1otHashes111111111111111111111111111");

/// How many slots `SlotHashes` remembers. A dispute's entropy lives exactly
/// that long — about 3.5 minutes — and this is the upper bound on the delay
/// between opening a dispute and selecting its panel.
pub const SLOT_HASHES_DEPTH: usize = 512;

/// Selection entropy. The type is opaque on purpose: all the rest of the program knows is
/// that the panel is derived deterministically from something, and where exactly that
/// "something" comes from is not visible outside this module.
pub struct Entropy([u8; 32]);

impl Entropy {
    /// A stream of numbers from a single seed. Hashing with a counter instead
    /// of chaining `state = hash(state)` — so that step `i` does not depend on
    /// how many steps came before it: this keeps `draw` a pure function of the
    /// seed and the step number.
    fn at(&self, step: u8) -> u64 {
        let bytes = hashv(&[self.0.as_slice(), &[step]]).0;
        u64::from_le_bytes(bytes[..8].try_into().expect("keccak gives 32 bytes"))
    }
}

/// The current entropy source — `FR-006`: a recent slot hash combined with the
/// dispute identifier. **This is the function a move to VRF replaces.**
///
/// The dispute identifier is here not for randomness but so that two disputes
/// opened in the same slot do not get the same panel.
pub fn entropy_of(slot_hashes: &AccountInfo, slot: u64, dispute: &Pubkey) -> Result<Entropy> {
    let data = slot_hashes.try_borrow_data()?;
    let slot_hash = find_slot_hash(&data, slot).ok_or(VerdictMeshError::EntropyUnavailable)?;

    Ok(Entropy(hashv(&[slot_hash.as_slice(), dispute.as_ref()]).0))
}

/// The hash of a specific slot, read from the sysvar's raw bytes.
///
/// `SlotHashes` is not deserialized in full: it is 512 entries, and Anchor
/// would spend both compute budget and stack on them. The layout is fixed —
/// an 8-byte count, then "slot, hash" pairs in **descending** slot order — so
/// a binary search reads exactly what it needs.
fn find_slot_hash(data: &[u8], slot: u64) -> Option<[u8; 32]> {
    let count = usize::try_from(u64::from_le_bytes(data.get(..8)?.try_into().ok()?)).ok()?;

    let (mut low, mut high) = (0usize, count);
    while low < high {
        let middle = low + (high - low) / 2;
        let at = 8 + middle.checked_mul(SLOT_HASH_ENTRY)?;
        let candidate = u64::from_le_bytes(data.get(at..at + 8)?.try_into().ok()?);

        match candidate.cmp(&slot) {
            Ordering::Equal => return data.get(at + 8..at + SLOT_HASH_ENTRY)?.try_into().ok(),
            // Descending order: a larger slot means the target lies further right.
            Ordering::Greater => low = middle + 1,
            Ordering::Less => high = middle,
        }
    }

    None
}

/// Picks `size` distinct positions from `0..candidates` — deterministically and
/// without repeats.
///
/// A partial Fisher–Yates shuffle: at step `i`, one of the not-yet-picked entries is
/// swapped into position `i`. Repeats are impossible by construction, and this matters
/// more than it seems — the same juror twice on a panel would mean a double-weight vote
/// and a quorum assembled by one person.
///
/// Modulo bias does exist here, but with `span ≤ 32` and a 64-bit number it is
/// on the order of 2⁻⁵⁹ — several orders of magnitude smaller than the slot
/// leader's influence on the hash itself, so it is not the weak link.
pub fn draw(entropy: &Entropy, candidates: u32, size: u8) -> Result<Vec<u32>> {
    require!(size > 0, VerdictMeshError::InvalidPolicy);
    require!(size <= MAX_PANEL_SIZE, VerdictMeshError::InvalidPolicy);
    require!(
        u64::from(size) <= u64::from(candidates),
        VerdictMeshError::RegistryTooSmall
    );

    let mut pool: Vec<u32> = (0..candidates).collect();
    for step in 0..size {
        let taken = u32::from(step);
        let span = candidates - taken;
        let pick = taken + u32::try_from(entropy.at(step) % u64::from(span)).expect("pick < span");
        pool.swap(taken as usize, pick as usize);
    }

    pool.truncate(usize::from(size));
    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entropy(seed: u8) -> Entropy {
        Entropy([seed; 32])
    }

    /// A panel without repeats is the key property. The same juror twice would
    /// mean a double-weight vote and a quorum assembled by one person.
    #[test]
    fn draws_distinct_positions_inside_the_registry() {
        for seed in 0..32u8 {
            let panel = draw(&entropy(seed), 12, 5).expect("a panel of 5 out of 12 exists");

            assert_eq!(panel.len(), 5);
            assert!(panel.iter().all(|position| *position < 12));

            let mut sorted = panel.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted.len(), panel.len(), "repeat in panel: {panel:?}");
        }
    }

    /// Determinism is a requirement, not a testing convenience: without it the
    /// selection can be neither verified nor reproduced by an outside party.
    #[test]
    fn gives_the_same_panel_for_the_same_entropy() {
        let first = draw(&entropy(7), 20, 5).expect("a panel exists");
        let second = draw(&entropy(7), 20, 5).expect("a panel exists");
        assert_eq!(first, second);
    }

    #[test]
    fn gives_different_panels_for_different_entropy() {
        let first = draw(&entropy(1), 20, 5).expect("a panel exists");
        let second = draw(&entropy(2), 20, 5).expect("a panel exists");
        assert_ne!(first, second);
    }

    /// No registry position is ever structurally unreachable. An off-by-one in
    /// the shuffle bounds usually looks exactly like this: selection works, the
    /// panel has no repeats, but the last juror never gets onto it.
    #[test]
    fn can_reach_every_position_in_the_registry() {
        let candidates = 9u32;
        let mut seen = vec![false; candidates as usize];

        for seed in 0..64u8 {
            for position in draw(&entropy(seed), candidates, 3).expect("a panel exists") {
                seen[position as usize] = true;
            }
        }

        assert!(
            seen.iter().all(|hit| *hit),
            "unreachable positions: {:?}",
            seen.iter()
                .enumerate()
                .filter(|(_, hit)| !**hit)
                .map(|(position, _)| position)
                .collect::<Vec<_>>()
        );
    }

    /// A panel the size of the registry is a permutation, not "almost everyone".
    #[test]
    fn a_panel_the_size_of_the_registry_takes_everyone() {
        let mut panel = draw(&entropy(3), 6, 6).expect("a panel exists");
        panel.sort_unstable();
        assert_eq!(panel, (0..6).collect::<Vec<u32>>());
    }

    /// A registry smaller than the panel means a refusal, not "a smaller panel":
    /// `FR-010` computes the quorum from the panel size in the policy.
    #[test]
    fn refuses_a_registry_smaller_than_the_panel() {
        assert!(draw(&entropy(1), 2, 3).is_err());
        assert!(draw(&entropy(1), 0, 1).is_err());
    }

    #[test]
    fn refuses_a_panel_beyond_the_cap() {
        assert!(draw(&entropy(1), 100, MAX_PANEL_SIZE + 1).is_err());
        assert!(draw(&entropy(1), 100, 0).is_err());
    }

    // ── sysvar ──────────────────────────────────────────────────────────────

    /// Sysvar bytes in the real layout: the count, then "slot, hash" pairs in
    /// descending slot order.
    fn slot_hashes(newest: u64, count: u64) -> Vec<u8> {
        let mut data = count.to_le_bytes().to_vec();
        for offset in 0..count {
            let slot = newest - offset;
            data.extend_from_slice(&slot.to_le_bytes());
            data.extend_from_slice(&[slot as u8; 32]);
        }
        data
    }

    #[test]
    fn finds_a_slot_hash_anywhere_in_the_sysvar() {
        let data = slot_hashes(1_000, 512);

        for slot in [1_000u64, 999, 744, 490, 489] {
            assert_eq!(
                find_slot_hash(&data, slot),
                Some([slot as u8; 32]),
                "{slot}"
            );
        }
    }

    /// A slot beyond the sysvar's depth means a refusal, not someone else's hash. A silent
    /// miss would yield a panel derived from something other than what was promised.
    #[test]
    fn refuses_a_slot_outside_the_sysvar() {
        let data = slot_hashes(1_000, 512);
        assert_eq!(find_slot_hash(&data, 488), None);
        assert_eq!(find_slot_hash(&data, 1_001), None);
    }

    /// A truncated or empty sysvar must not lead to reading past the end of the
    /// buffer: a bug here is a program panic on a live dispute.
    #[test]
    fn survives_a_truncated_sysvar() {
        assert_eq!(find_slot_hash(&[], 1), None);
        assert_eq!(find_slot_hash(&0u64.to_le_bytes(), 1), None);

        let mut lying = slot_hashes(1_000, 4);
        lying[..8].copy_from_slice(&512u64.to_le_bytes());
        assert_eq!(find_slot_hash(&lying, 1), None);
    }
}
