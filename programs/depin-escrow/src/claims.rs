//! Fingerprints of the parties' offers — `FR-005`.
//!
//! **Here a position is a number, not a role.** In a milestone deal there are
//! exactly two positions and the role implies which one a party holds
//! (`reference_escrow::claims`). Here the network says "the operator earned
//! `reported_bps`" and the operator says "I earned `claimed_bps`": the claim is
//! the offer itself, and the verdict picks one of the two (final-offer
//! arbitration). Both fingerprints are still computed by the program from
//! what is in the epoch's state — the claimant never words the respondent's
//! position.
//!
//! Off-chain code recomputes the same formula to show a juror what each side
//! signed up to and to prove it matches the dispute; there are 10 001 possible
//! offers per epoch, so the text is recoverable from the fingerprint alone.

use anchor_lang::prelude::*;
use solana_keccak_hasher::hashv;

/// Domain separator. The version is in the string so that a change of scheme
/// is a visible change, not a silent mismatch with disputes still under review.
const DOMAIN: &[u8] = b"depin_escrow/claim/v1";

/// The fingerprint of "the operator's share of `epoch` is `operator_bps`".
pub fn claim_of(epoch: &Pubkey, operator_bps: u16) -> [u8; 32] {
    hashv(&[DOMAIN, epoch.as_ref(), &operator_bps.to_le_bytes()]).0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `open_dispute` refuses a zero fingerprint as a missing claim.
    #[test]
    fn never_produces_an_empty_claim() {
        let epoch = Pubkey::new_unique();
        for bps in [0, 1, 5_000, 10_000] {
            assert_ne!(claim_of(&epoch, bps), [0u8; 32]);
        }
    }

    #[test]
    fn differs_by_offer_and_by_epoch() {
        let epoch = Pubkey::new_unique();
        assert_ne!(claim_of(&epoch, 6_000), claim_of(&epoch, 9_000));
        assert_ne!(
            claim_of(&epoch, 6_000),
            claim_of(&Pubkey::new_unique(), 6_000)
        );
    }

    /// The byte layout is what the off-chain decoder hashes; pin it.
    #[test]
    fn hashes_the_share_as_two_little_endian_bytes() {
        let epoch = Pubkey::new_from_array([7u8; 32]);
        let expected = hashv(&[DOMAIN, &[7u8; 32], &[0x28, 0x23]]).0;
        assert_eq!(claim_of(&epoch, 9_000), expected);
    }
}
