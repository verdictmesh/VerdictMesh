import { keccak_256 } from '@noble/hashes/sha3'
import type { PublicKey } from '@solana/web3.js'

/**
 * The sealed vote. A juror commits to a hash first and opens it after the
 * commit window closes (`FR-008a`); the program recomputes the hash on reveal
 * and refuses one that does not match.
 *
 * The formula is `programs/verdict-mesh/src/vote.rs` → `commitment_of`, and
 * this is its only TypeScript copy — the panel and the runs import it from
 * here. Two copies do not fail to compile; they fail on reveal, and the juror
 * whose vote cannot be opened is slashed for a silence they did not choose.
 * Like `./pda`, this module does not import Anchor.
 */

/** `vote.rs` → `DOMAIN`. */
const DOMAIN = new TextEncoder().encode('verdict_mesh/vote/v1')

/** The two sides a juror can vote for — the variant names of `Ballot` in the IDL. */
export type Ballot = 'Claimant' | 'Respondent'

/** `vote.rs` → `tag`: numbered from one, so zeroed memory is never a vote. */
const TAG: Record<Ballot, number> = { Claimant: 1, Respondent: 2 }

export const SALT_LEN = 32

/** The ballot as Anchor's `methods.revealVote(...)` takes an enum argument. */
export const ballotArg = (choice: Ballot) =>
  choice === 'Claimant' ? { claimant: {} } : { respondent: {} }

const concat = (...parts: Uint8Array[]) => {
  const out = new Uint8Array(parts.reduce((n, part) => n + part.length, 0))
  let offset = 0
  for (const part of parts) {
    out.set(part, offset)
    offset += part.length
  }
  return out
}

/**
 * The commitment to `choice` by `juror` (the wallet) in `dispute`. Both keys
 * are in the hash: without the juror, a commitment copied into another
 * account would reveal with the author's salt — a vote cast without any work.
 */
export function commitmentOf(
  dispute: PublicKey,
  juror: PublicKey,
  choice: Ballot,
  salt: Uint8Array,
): Uint8Array {
  if (salt.length !== SALT_LEN) throw new Error(`The salt must be ${SALT_LEN} bytes`)
  return keccak_256(
    concat(DOMAIN, dispute.toBytes(), juror.toBytes(), Uint8Array.of(TAG[choice]), salt),
  )
}
