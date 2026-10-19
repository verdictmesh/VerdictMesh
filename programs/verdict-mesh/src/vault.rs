//! Review fee — `FR-026`, `FR-026a`, `FR-026b`, `FR-026c`.
//!
//! **The deposit is the price of the review.** The policy holds a single number,
//! `Policy.deposit`, and that same number is announced to the party before the dispute is opened
//! (`FR-026d`). There is no second number from which a "review cost" could be computed separately
//! from the deposit, and inventing one would give the integrator a way to announce one amount and
//! charge another.
//!
//! **The deposit is always paid by the initiator, and always up front.** It sits in the dispute
//! vault from the moment of opening, so settlement splits what is already there and does not
//! depend on whether a third-party program has run yet. The question of "who ultimately bore the
//! cost" (`FR-026a`) is decided by the **escrow** at payout: if the initiator won, the escrow
//! withholds this amount from the losing party's share and gives it to the initiator. Otherwise
//! we would have to require the escrow to transfer the fee **before** our settlement — that is,
//! make the jurors' payout depend on execution order in a third-party program we have no
//! authority over (`FR-012`).
//!
//! **The protocol is last in line.** The jurors' share is computed from the announced price and
//! capped by what was actually collected; the protocol takes the remainder. This is the only
//! ordering in which a shortfall does not fall on the juror: a juror counts their earnings in
//! advance and has no way to check how much reached the vault
//! (`FR-026c`).

use anchor_lang::prelude::*;

use crate::{errors::VerdictMeshError, state::BPS_DENOMINATOR};

/// The jurors' share of the review fee — 80%; the rest goes to the protocol.
///
/// **A protocol constant, not a `Policy` field.** The integrator has no grounds to decide how the
/// protocol pays its own jurors: there is one registry for everyone, and a policy that gives
/// jurors zero would make panel service unprofitable across **all** integrators at once — a
/// single dispute under such a policy is enough for the selected panel to work for free. The
/// integrator sets the deposit amount itself (`FR-026d`); splitting what has been collected is
/// the protocol's business.
pub const JUROR_FEE_BPS: u16 = 8_000;

/// How the collected review fee is split — `FR-026b`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FeeSplit {
    pub jurors: u64,
    pub protocol: u64,
}

/// Splits the dispute vault between the jurors and the protocol.
///
/// `balance` is how much was actually collected, `cost` is the announced review price. The two
/// always match as long as the vault holds exactly the deposit, and diverge only where something
/// went wrong; in that case the protocol is short-changed, not the juror.
///
/// `has_winners` says whether there is anyone to pay the jurors' share to. A panel that stayed
/// entirely silent has no winners (T020), and the share meant for them has nowhere to land: the
/// instruction closes the dispute vault, so leaving the amount there would destroy it together
/// with the account.
pub fn split_fee(balance: u64, cost: u64, has_winners: bool) -> Result<FeeSplit> {
    let jurors = if has_winners {
        share_of(cost, JUROR_FEE_BPS)?.min(balance)
    } else {
        0
    };

    Ok(FeeSplit {
        jurors,
        // The subtraction always succeeds: the jurors' share is capped by the balance itself.
        protocol: balance
            .checked_sub(jurors)
            .ok_or(VerdictMeshError::Overflow)?,
    })
}

/// A share of an amount in basis points. Computed in `u128` because a `u64` times ten thousand
/// does not fit in `u64` — and this is money, not a counter.
///
/// Rounds down: the remainder goes to whoever comes last in their settlement. Rounding up would
/// pay out more than was collected, and the first to notice would be whoever
/// came up short.
pub fn share_of(amount: u64, bps: u16) -> Result<u64> {
    let share = u128::from(amount)
        .checked_mul(u128::from(bps))
        .ok_or(VerdictMeshError::Overflow)?
        / u128::from(BPS_DENOMINATOR);

    u64::try_from(share).map_err(|_| VerdictMeshError::Overflow.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Demo configuration: a 5 USDC deposit, 6 decimals.
    const COST: u64 = 5_000_000;

    #[test]
    fn splits_the_declared_cost_four_to_one() {
        let split = split_fee(COST, COST, true).unwrap();

        assert_eq!(split.jurors, 4_000_000);
        assert_eq!(split.protocol, 1_000_000);
    }

    /// `FR-026c`: a shortfall falls on the protocol, not on the juror. A juror counts their
    /// earnings in advance and has no way to check how much arrived.
    #[test]
    fn a_short_balance_costs_the_protocol_first() {
        let split = split_fee(4_500_000, COST, true).unwrap();

        assert_eq!(split.jurors, 4_000_000);
        assert_eq!(split.protocol, 500_000);
    }

    /// A shortfall deeper than the protocol's share cuts into the jurors' share too — but only
    /// after the protocol has lost all of its own.
    #[test]
    fn a_balance_below_the_juror_share_leaves_the_protocol_nothing() {
        let split = split_fee(1_000_000, COST, true).unwrap();

        assert_eq!(split.jurors, 1_000_000);
        assert_eq!(split.protocol, 0);
    }

    /// A panel that stayed entirely silent: there are no winners, and the jurors' share has
    /// nowhere to land. It goes to the protocol instead of staying in a vault that is being
    /// closed.
    #[test]
    fn without_winners_the_whole_payment_goes_to_the_protocol() {
        let split = split_fee(COST, COST, false).unwrap();

        assert_eq!(split.jurors, 0);
        assert_eq!(split.protocol, COST);
    }

    #[test]
    fn an_empty_vault_splits_into_nothing() {
        assert_eq!(
            split_fee(0, COST, true).unwrap(),
            FeeSplit {
                jurors: 0,
                protocol: 0
            }
        );
    }

    /// The split creates nothing and loses nothing: the shares add up to the balance for any
    /// input. This is the equality that lets the vault be closed right after
    /// settlement.
    #[test]
    fn the_two_shares_always_add_up_to_the_balance() {
        for balance in [0, 1, 2, 3, 7, 999, 1_000_000, COST, u64::MAX] {
            for has_winners in [true, false] {
                let split = split_fee(balance, COST, has_winners).unwrap();

                assert_eq!(
                    split.jurors.checked_add(split.protocol),
                    Some(balance),
                    "balance {balance}, winners {has_winners}"
                );
            }
        }
    }

    /// The dust from an uneven split goes to the protocol: it is the residual recipient.
    /// The jurors were promised a share of the announced price, and it is paid
    /// in full.
    #[test]
    fn the_rounding_dust_goes_to_the_protocol() {
        let split = split_fee(3, 3, true).unwrap();

        // 3 * 8000 / 10000 = 2.4 → 2.
        assert_eq!(split.jurors, 2);
        assert_eq!(split.protocol, 1);
    }

    #[test]
    fn a_full_share_of_the_largest_amount_does_not_overflow() {
        assert_eq!(share_of(u64::MAX, BPS_DENOMINATOR).unwrap(), u64::MAX);
    }

    #[test]
    fn a_zero_share_is_zero() {
        assert_eq!(share_of(u64::MAX, 0).unwrap(), 0);
    }
}
