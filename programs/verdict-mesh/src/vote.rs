//! Juror vote commitment — `FR-008`.
//!
//! This module lives apart from the instructions for one reason: **the commitment is
//! computed by two parties**. The juror computes it off-chain to submit it (T017), and
//! the program computes it again on reveal to verify it (`FR-008a`, T018). Two copies
//! of the formula do not cause a compile error — they cause a vote that cannot be
//! revealed, and a juror slashed for a silence they never chose.
//!
//! **What goes into the commitment, and why exactly this.**
//!
//! `juror` — so that someone else's commitment cannot be copied. Without binding to the
//! wallet, replaying another juror's `commitment` in your own account costs nothing:
//! the copycat would wait for the author to reveal and then reveal with the same
//! secret. That is a vote with no work and no risk of being wrong — exactly what
//! commit-reveal was meant to prevent.
//!
//! `dispute` — so that a commitment does not carry over between disputes. The same
//! choice with the same secret in another dispute must yield a different value;
//! otherwise a juror who votes in many disputes reveals their previous secret in each
//! subsequent one.
//!
//! `DOMAIN` — so that a hash computed elsewhere in the protocol (a report fingerprint,
//! a party's claim) cannot accidentally collide with a vote commitment. It is cheap
//! caution now and the only possible caution later.
//!
//! `salt` — what actually hides the vote: there are exactly two options, and without a
//! secret the commitment is brute-forced on the first try.

use anchor_lang::prelude::*;
use solana_keccak_hasher::hashv;

use crate::state::Ballot;

/// Hash domain separator. The version is in the string so that a scheme change is a
/// visible change rather than a silent mismatch with old, not-yet-revealed votes.
const DOMAIN: &[u8] = b"verdict_mesh/vote/v1";

/// Secret length. 32 bytes is not "just in case": a shorter secret can be brute-forced,
/// and a brute-forced secret exposes the vote before the reveal window (`FR-009`).
pub const SALT_LEN: usize = 32;

/// Vote commitment. **This is the formula the SDK replicates** (T035) — there is no
/// other source of truth for it.
pub fn commitment_of(
    dispute: &Pubkey,
    juror: &Pubkey,
    choice: Ballot,
    salt: &[u8; SALT_LEN],
) -> [u8; 32] {
    hashv(&[
        DOMAIN,
        dispute.as_ref(),
        juror.as_ref(),
        &[tag(choice)],
        salt,
    ])
    .0
}

/// Choice byte. Numbering starts at one: zero stays a value that does not exist,
/// so zeroed memory never turns into a valid vote.
///
/// The match has no `_` on purpose: a new `Ballot` variant must stop compilation here
/// rather than silently get another variant's byte and collapse two different votes
/// into one commitment.
fn tag(choice: Ballot) -> u8 {
    match choice {
        Ballot::Claimant => 1,
        Ballot::Respondent => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn salt(byte: u8) -> [u8; SALT_LEN] {
        [byte; SALT_LEN]
    }

    /// The foundation of everything: the same four inputs give the same value.
    /// Otherwise not even the juror who submitted a vote could reveal it.
    #[test]
    fn the_same_inputs_give_the_same_commitment() {
        let dispute = Pubkey::new_unique();
        let juror = Pubkey::new_unique();

        assert_eq!(
            commitment_of(&dispute, &juror, Ballot::Claimant, &salt(7)),
            commitment_of(&dispute, &juror, Ballot::Claimant, &salt(7)),
        );
    }

    /// The module's key check. A copied commitment must be useless: in someone
    /// else's account it matches nothing the copycat can reveal — even after the
    /// author has shown their secret to the whole world.
    #[test]
    fn a_copied_commitment_belongs_to_no_one_else() {
        let dispute = Pubkey::new_unique();
        let author = Pubkey::new_unique();
        let copycat = Pubkey::new_unique();

        assert_ne!(
            commitment_of(&dispute, &author, Ballot::Claimant, &salt(7)),
            commitment_of(&dispute, &copycat, Ballot::Claimant, &salt(7)),
        );
    }

    /// A secret revealed in one dispute does not unlock a vote in another.
    #[test]
    fn the_same_secret_gives_a_different_commitment_in_another_dispute() {
        let juror = Pubkey::new_unique();

        assert_ne!(
            commitment_of(&Pubkey::new_unique(), &juror, Ballot::Claimant, &salt(7)),
            commitment_of(&Pubkey::new_unique(), &juror, Ballot::Claimant, &salt(7)),
        );
    }

    /// The choice must be in the hash: otherwise the commitment binds to nothing
    /// and can be revealed as any vote.
    #[test]
    fn every_choice_gives_its_own_commitment() {
        let dispute = Pubkey::new_unique();
        let juror = Pubkey::new_unique();

        let commitments: Vec<[u8; 32]> = [Ballot::Claimant, Ballot::Respondent]
            .into_iter()
            .map(|choice| commitment_of(&dispute, &juror, choice, &salt(7)))
            .collect();

        for (position, commitment) in commitments.iter().enumerate() {
            assert!(
                !commitments[position + 1..].contains(commitment),
                "two different choices share a commitment"
            );
        }
    }

    /// Without this the vote is hidden only from those who did not think to try
    /// both options.
    #[test]
    fn a_different_secret_hides_the_same_choice_differently() {
        let dispute = Pubkey::new_unique();
        let juror = Pubkey::new_unique();

        assert_ne!(
            commitment_of(&dispute, &juror, Ballot::Claimant, &salt(7)),
            commitment_of(&dispute, &juror, Ballot::Claimant, &salt(8)),
        );
    }

    /// The vote cannot be guessed in two attempts while the secret is unknown: no
    /// ballot option with a **different** secret matches the commitment.
    #[test]
    fn guessing_the_choice_without_the_secret_matches_nothing() {
        let dispute = Pubkey::new_unique();
        let juror = Pubkey::new_unique();
        let real = commitment_of(&dispute, &juror, Ballot::Respondent, &salt(7));

        for choice in [Ballot::Claimant, Ballot::Respondent] {
            assert_ne!(real, commitment_of(&dispute, &juror, choice, &salt(0)));
        }
    }
}
