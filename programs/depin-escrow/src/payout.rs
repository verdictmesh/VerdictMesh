//! How the epoch's reward is split — `FR-026a`, `FR-026f`.
//!
//! Pure arithmetic, apart from the instructions that move the tokens, so that
//! the one property everything rests on — the escrow keeps nothing and mints
//! nothing, `operator + network == reward` — is tested directly.

use anchor_lang::prelude::*;

use crate::{errors::DepinError, state::FULL_SHARE};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Payout {
    pub operator: u64,
    pub network: u64,
    /// How much of the network's share went to the operator to make good the
    /// review deposit. Zero unless the operator won a dispute.
    pub reimbursed: u64,
}

impl Payout {
    /// `reward` split at `operator_bps`. The operator's share is rounded down
    /// and the dust stays with the network, which funded the reward: rounding
    /// the other way would pay out a unit nobody locked.
    pub fn at(reward: u64, operator_bps: u16) -> Result<Self> {
        require!(operator_bps <= FULL_SHARE, DepinError::InvalidShare);
        let operator =
            u64::try_from(u128::from(reward) * u128::from(operator_bps) / u128::from(FULL_SHARE))
                .map_err(|_| DepinError::Overflow)?;

        Ok(Self {
            operator,
            network: reward - operator,
            reimbursed: 0,
        })
    }

    /// The operator won: the network, against whom the verdict went, bears the
    /// cost of the review (`FR-026a`). The deposit comes out of the network's
    /// share — the source a milestone escrow lacks, since there the loser gets
    /// nothing. If that share is smaller than the deposit, the operator is
    /// made good for what there is and the rest is charged to nobody
    /// (`FR-026f`); the operator's share under the verdict itself never shrinks.
    pub fn reimbursing_operator(self, deposit: u64) -> Result<Self> {
        let reimbursed = deposit.min(self.network);
        Ok(Self {
            operator: self
                .operator
                .checked_add(reimbursed)
                .ok_or(DepinError::Overflow)?,
            network: self.network - reimbursed,
            reimbursed,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REWARD: u64 = 100_000_000;

    #[test]
    fn splits_the_reward_at_the_share() {
        assert_eq!(
            Payout::at(REWARD, 6_000).unwrap(),
            Payout {
                operator: 60_000_000,
                network: 40_000_000,
                reimbursed: 0,
            }
        );
        assert_eq!(Payout::at(REWARD, 0).unwrap().network, REWARD);
        assert_eq!(Payout::at(REWARD, FULL_SHARE).unwrap().operator, REWARD);
    }

    #[test]
    fn leaves_the_rounding_dust_with_the_network() {
        let payout = Payout::at(7, 5_000).unwrap();
        assert_eq!((payout.operator, payout.network), (3, 4));
    }

    #[test]
    fn refuses_a_share_above_the_whole() {
        assert!(Payout::at(REWARD, FULL_SHARE + 1).is_err());
    }

    #[test]
    fn does_not_overflow_on_the_largest_reward() {
        let payout = Payout::at(u64::MAX, 9_999).unwrap();
        assert_eq!(payout.operator + payout.network, u64::MAX);
    }

    #[test]
    fn reimburses_the_deposit_out_of_the_network_share() {
        let payout = Payout::at(REWARD, 9_000)
            .unwrap()
            .reimbursing_operator(5_000_000)
            .unwrap();
        assert_eq!(
            payout,
            Payout {
                operator: 95_000_000,
                network: 5_000_000,
                reimbursed: 5_000_000,
            }
        );
    }

    /// `FR-026f`: the network's share is all there is to take from.
    #[test]
    fn reimburses_no_more_than_the_network_share() {
        let payout = Payout::at(REWARD, 9_800)
            .unwrap()
            .reimbursing_operator(5_000_000)
            .unwrap();
        assert_eq!(payout.reimbursed, 2_000_000);
        assert_eq!((payout.operator, payout.network), (REWARD, 0));
    }

    #[test]
    fn never_keeps_or_mints_a_unit() {
        for reward in [1, 7, 999, REWARD, u64::MAX / 2] {
            for bps in [0, 1, 3_333, 5_000, 9_999, FULL_SHARE] {
                for deposit in [0, 1, 5_000_000, u64::MAX] {
                    let plain = Payout::at(reward, bps).unwrap();
                    assert_eq!(plain.operator + plain.network, reward);
                    let paid = plain.reimbursing_operator(deposit).unwrap();
                    assert_eq!(paid.operator + paid.network, reward);
                }
            }
        }
    }
}
