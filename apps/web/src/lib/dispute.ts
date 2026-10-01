import type { DisputeView, Verdict } from '@verdictmesh/shared'

/**
 * What a dispute is doing right now, as a reader needs to see it.
 *
 * The program moves between windows by the clock, not by an instruction:
 * `commit_vote` is accepted until `commit_deadline`, `reveal_vote` from then
 * until `reveal_deadline`, and the state on chain only catches up when someone
 * sends a transaction. So the stage is derived from the deadlines and the
 * verdict, never read off `state` alone — a dispute still `Committing` on
 * chain an hour after its commit window is not taking votes.
 *
 * - `optimistic` — no panel yet (the optimistic track, `FR-024`).
 * - `commit`, `reveal` — the two voting windows.
 * - `tally` — both windows closed, nobody has sent `tally` yet.
 * - `appeal` — a verdict is recorded and the appeal window is open.
 * - `final` — the verdict stands; the escrow has not been seen paying out.
 * - `settled` — the escrow's settlement transaction is known.
 */
export type Stage = 'optimistic' | 'commit' | 'reveal' | 'tally' | 'appeal' | 'final' | 'settled'

export const STAGE_LABEL: Record<Stage, string> = {
  optimistic: 'Optimistic track',
  commit: 'Commit window',
  reveal: 'Reveal window',
  tally: 'Awaiting tally',
  appeal: 'Appeal window',
  final: 'Verdict final',
  settled: 'Settled',
}

type StageInput = Pick<
  DisputeView,
  'state' | 'verdict' | 'commitDeadline' | 'revealDeadline' | 'appealDeadline' | 'settlement'
>

/** `nowSec` — unix seconds. */
export function stageOf(dispute: StageInput, nowSec: number): Stage {
  if (dispute.state === 'OptimisticPending') return 'optimistic'
  if (dispute.verdict === null) {
    if (nowSec < dispute.commitDeadline) return 'commit'
    if (nowSec < dispute.revealDeadline) return 'reveal'
    return 'tally'
  }
  if (dispute.settlement.status === 'settled') return 'settled'
  if (nowSec < dispute.appealDeadline) return 'appeal'
  return 'final'
}

/** The deadline the current stage runs to, in unix seconds; `null` — none. */
export function stageDeadline(dispute: StageInput, stage: Stage): number | null {
  if (stage === 'commit') return dispute.commitDeadline
  if (stage === 'reveal') return dispute.revealDeadline
  if (stage === 'appeal') return dispute.appealDeadline
  return null
}

/** Stages in which a vote can still change something. */
export const OPEN_STAGES: ReadonlySet<Stage> = new Set(['commit', 'reveal'])

/** Base units per whole unit of the settlement asset: the demo mint has 6 decimals. */
const DECIMALS = 6n
const UNIT = 10n ** DECIMALS

/**
 * A `u64` amount in base units, as `DisputeView` carries it, to a display
 * string. Kept in `bigint` all the way: a `number` loses the low digits past
 * 2^53, and those are somebody's money.
 */
export function formatAmount(baseUnits: string): string {
  const value = BigInt(baseUnits)
  const whole = value / UNIT
  const fraction = (value % UNIT).toString().padStart(Number(DECIMALS), '0')
  const trimmed = fraction.replace(/0+$/, '').padEnd(2, '0')
  return `${whole.toLocaleString('en-US')}.${trimmed} USDC`
}

export function shortenMiddle(value: string, lead = 4, tail = 4): string {
  if (value.length <= lead + tail + 1) return value
  return `${value.slice(0, lead)}…${value.slice(-tail)}`
}

export function formatDuration(ms: number): string {
  const total = Math.max(0, Math.ceil(ms / 1000))
  const h = Math.floor(total / 3600)
  const m = Math.floor((total % 3600) / 60)
  const s = total % 60
  if (h > 0) return `${h}h ${String(m).padStart(2, '0')}m`
  if (m > 0) return `${m}m ${String(s).padStart(2, '0')}s`
  return `${s}s`
}

/** "3 minutes ago", coarse on purpose: the exact time is one click away on chain. */
export function formatAgo(thenSec: number, nowSec: number): string {
  const seconds = Math.max(0, nowSec - thenSec)
  if (seconds < 60) return 'just now'
  const units: [number, string][] = [
    [86_400, 'day'],
    [3_600, 'hour'],
    [60, 'minute'],
  ]
  for (const [size, name] of units) {
    if (seconds >= size) {
      const n = Math.floor(seconds / size)
      return `${n} ${name}${n === 1 ? '' : 's'} ago`
    }
  }
  return 'just now'
}

export const VERDICT_LABEL: Record<Verdict, string> = {
  Claimant: 'For the claimant',
  Respondent: 'For the respondent',
  StatusQuo: 'Status quo — funds go back where they came from',
}

/** Who the escrow pays under a verdict. `null` — nobody moves: status quo. */
export function recipientOf(dispute: Pick<DisputeView, 'verdict' | 'claimant' | 'respondent'>) {
  if (dispute.verdict === 'Claimant') return dispute.claimant
  if (dispute.verdict === 'Respondent') return dispute.respondent
  return null
}

const CLUSTER = 'devnet'

export const explorerTx = (signature: string) =>
  `https://explorer.solana.com/tx/${signature}?cluster=${CLUSTER}`

export const explorerAddress = (address: string) =>
  `https://explorer.solana.com/address/${address}?cluster=${CLUSTER}`
