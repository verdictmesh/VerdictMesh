import type { IdlAccounts } from '@coral-xyz/anchor'
import type { PublicKey } from '@solana/web3.js'
import type { VerdictMesh } from './idl/verdict_mesh.js'

/**
 * When an escrow may execute a verdict (`FR-012`), seen from off chain.
 *
 * The escrow decides on chain with `Dispute::ruling` from
 * `programs/verdict-mesh/src/verdict_read.rs`; this is the same rule, so that
 * a crank or a page knows whether a settlement transaction would pass before
 * sending it. Both are checked against one table,
 * `fixtures/verdict-read/cases.json`. Nothing here imports Anchor at run time:
 * the dispute comes in decoded, by `VerdictMeshClient.readVerdict` or by
 * whatever the caller already uses.
 */

/** A `Dispute` account as Anchor decodes it. */
export type Dispute = IdlAccounts<VerdictMesh>['dispute']

export type Verdict = 'claimant' | 'respondent' | 'statusQuo'

/**
 * `ready` — the escrow may execute `verdict` now. Every other status is named
 * after the `VerdictReadError` the escrow's settlement would fail with.
 */
export type VerdictStatus =
  | {
      status: 'ready'
      verdict: Verdict
      /** The party whose position won; `null` on `statusQuo`, when nothing moves (`FR-027a`). */
      winner: PublicKey | null
      /** The review deposit from the dispute's snapshot of the policy (`FR-003`, `FR-026e`). */
      deposit: bigint
    }
  | { status: 'NotOurDispute' }
  | { status: 'VerdictPending' }
  | { status: 'VerdictUnderAppeal' }
  /** `until` — the appeal deadline, unix seconds. */
  | { status: 'AppealWindowOpen'; until: number }

type VerdictInput = Pick<
  Dispute,
  'escrowRef' | 'state' | 'verdict' | 'appealDeadline' | 'claimant' | 'respondent' | 'policy'
>

/** Anchor decodes a Rust enum as an object with one key: `{ tallied: {} }`. */
const variant = (value: object) => Object.keys(value)[0]

/**
 * The verdict `escrow` may execute at `nowSec`, or why not — in the order the
 * program checks: the dispute was opened over `escrow`; it has a verdict; it
 * is not under appeal; the appeal window has closed.
 *
 * `nowSec` should be the cluster's time, not this machine's: the program
 * compares against the `Clock` sysvar, and a local clock a few seconds ahead
 * reports `ready` for a transaction that fails. `readVerdict` takes it from
 * the cluster.
 *
 * `finalized` is not required, as on chain: it only says the panel's stakes
 * are settled, which is VerdictMesh's business, not the escrow's.
 */
export function verdictStatus(
  dispute: VerdictInput,
  escrow: PublicKey,
  nowSec: number,
): VerdictStatus {
  if (!dispute.escrowRef.equals(escrow)) return { status: 'NotOurDispute' }
  if (dispute.verdict === null) return { status: 'VerdictPending' }
  if (variant(dispute.state) === 'appealed') return { status: 'VerdictUnderAppeal' }
  const until = dispute.appealDeadline.toNumber()
  if (nowSec < until) return { status: 'AppealWindowOpen', until }

  const verdict = variant(dispute.verdict) as Verdict
  const winner =
    verdict === 'claimant' ? dispute.claimant : verdict === 'respondent' ? dispute.respondent : null
  return { status: 'ready', verdict, winner, deposit: BigInt(dispute.policy.deposit.toString()) }
}
