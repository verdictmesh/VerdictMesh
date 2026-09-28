import { keccak_256 } from '@noble/hashes/sha3'
import { PublicKey } from '@solana/web3.js'
import type { PositionDecoder } from './evidence.js'

/**
 * The positions of the parties of a `reference_escrow` dispute, recovered from
 * the fingerprints stored in the `Dispute` account — `FR-005`.
 *
 * **The chain holds fingerprints, not text, and the text is not ours to
 * invent.** The escrow derives both positions from the roles of the parties
 * (`programs/reference-escrow/src/claims.rs`): the seller wants the milestone
 * released, the buyer wants it refunded. So the text can be rebuilt — but only
 * by hashing every candidate and finding the one that matches the fingerprint
 * the escrow actually wrote. A statement is returned only on a match: a
 * position the report shows is then a position the dispute provably carries,
 * and a drift between the two copies of the formula turns into "not
 * recovered" instead of a confident wrong statement.
 *
 * This is the formula of one escrow. Another integrator's escrow words its
 * positions its own way, and without a decoder of its own its positions stay
 * fingerprints — which the report says out loud.
 */

/** The same domain separator as in `claims.rs`, version included. */
const DOMAIN = Buffer.from('reference_escrow/claim/v1')

export type EscrowPosition = 'release' | 'refund'

/** Tags from `Position::tag`. Zero is never used: a zero claim means "none". */
const TAG: Record<EscrowPosition, number> = { release: 1, refund: 2 }

/** A milestone index is a `u8` on chain; there cannot be more candidates. */
const MILESTONES = 256

/** `claim_of(escrow, milestone, position)` as lowercase hex. */
export function claimOf(escrow: PublicKey, milestone: number, position: EscrowPosition): string {
  if (!Number.isInteger(milestone) || milestone < 0 || milestone >= MILESTONES) {
    throw new Error(`Milestone index out of range: ${milestone}`)
  }

  return Buffer.from(
    keccak_256(Buffer.concat([DOMAIN, escrow.toBuffer(), Buffer.from([milestone, TAG[position]])])),
  ).toString('hex')
}

const STATEMENT: Record<EscrowPosition, (milestone: number) => string> = {
  release: (milestone) =>
    `Milestone #${milestone} was delivered, so its locked funds should be released to the seller.`,
  refund: (milestone) =>
    `Milestone #${milestone} was not delivered, so its locked funds should be refunded to the buyer.`,
}

/**
 * Every milestone times both positions — 512 hashes, well under a millisecond
 * each. Cheaper than reading the escrow to learn how many milestones it has,
 * and it does not depend on the account being decodable at all.
 */
export const referenceEscrowPositions: PositionDecoder = (escrowRef, fingerprint) => {
  const escrow = new PublicKey(escrowRef)

  for (let milestone = 0; milestone < MILESTONES; milestone += 1) {
    for (const position of ['release', 'refund'] as const) {
      if (claimOf(escrow, milestone, position) === fingerprint) {
        return STATEMENT[position](milestone)
      }
    }
  }

  return null
}
