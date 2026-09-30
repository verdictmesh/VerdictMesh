import { utils } from '@coral-xyz/anchor'
import type {
  ApiError,
  FactFindingReport,
  ReportResponse,
  ReportUnavailableReason,
} from '@verdictmesh/shared'
import { factFindingReport } from '@verdictmesh/shared'
import type { Context } from 'hono'
import { Hono } from 'hono'
import type { ContentfulStatusCode } from 'hono/utils/http-status'
import { z } from 'zod'
import { needsReport, reportHash, type TransientUnavailability } from '../reporter.js'
import { type Chain, type DisputeRow, disputeSnapshot } from '../watcher.js'

/**
 * `GET /disputes/:pda/report` — the one way to read a report (`FR-017b`).
 *
 * The route exists to answer "is this the report the jurors were promised?",
 * so both halves of that comparison are taken from where they cannot be
 * rewritten together:
 *
 * - **`hash` is computed from the body being served**, not read from
 *   `reports.content_hash`. A body edited in the database next to its untouched
 *   hash would otherwise be served with a hash that still matches the chain.
 *   Hashing what is returned also means a client that runs `sha256` over the
 *   canonical JSON of `report` gets exactly `hash`, whatever happened upstream.
 * - **the fingerprint is read from the `Dispute` account at request time**,
 *   not from the `disputes` mirror. The mirror lives in the same database as
 *   the body: a check of one table against another would be a check of the
 *   database against itself.
 *
 * When the chain cannot be read the report is not served at all. A report
 * without `matchesOnchain` is exactly what RLS forbids everyone else to read
 * (`docs/TASKS.md` → T026), and the route must not become that second path on
 * a bad RPC day.
 *
 * The account's owner is not checked: the address comes from our own table,
 * where only a PDA of this program can get, and nobody but the program can
 * create an account at its PDA.
 *
 * **No report is a status, not an error (`FR-018`).** The dispute goes on
 * without one, and the panel has to tell "coming" from "not coming". That
 * answer is taken from the mirror, not the chain: it makes no claim a reader
 * relies on against us — a wrong one only hides a report — and it costs no RPC
 * call. The final reasons are read off the dispute's state, which anyone can
 * check on chain; the transient ones come from the reporter in this process.
 * 404 is left for a dispute the mirror does not know.
 */

/** The published report of a dispute, as stored — not yet trusted. */
export interface PublishedReports {
  /**
   * The body of the first stored version — the one `attest_report` puts on
   * chain — as it came out of the database. `null` — no report yet.
   */
  first(disputePda: string): Promise<unknown>
}

/** Exactly the part of pino the route uses. */
export interface RouteLog {
  error(fields: Record<string, unknown>, message: string): void
}

/** What the status of a missing report is decided from. */
export type MirroredDispute = Pick<
  DisputeRow,
  'state' | 'escalated' | 'reportHash' | 'commitDeadline'
>

export interface MirroredDisputes {
  /** The mirror row of the dispute; `null` — the mirror does not know it. */
  find(disputePda: string): Promise<MirroredDispute | null>
}

export interface ReportRoutesOptions {
  reports: PublishedReports
  disputes: MirroredDisputes
  chain: Pick<Chain, 'readDispute'>
  /** What the reporter knows against a dispute right now. */
  reporter: { unavailable(disputePda: string): TransientUnavailability | null }
  log: RouteLog
  /** Milliseconds. */
  now?: () => number
}

type Missing = Exclude<ReportResponse, { status: 'ready' }>

const unavailable = (reason: ReportUnavailableReason, final: boolean): Missing => ({
  status: 'unavailable',
  reason,
  final,
})

/**
 * The status of a dispute whose report is not stored. The final reasons come
 * first: once the chain has settled that no report will be attested, what the
 * reporter last saw no longer matters.
 */
export function missingReportStatus(
  dispute: MirroredDispute,
  transient: TransientUnavailability | null,
  nowMs: number,
): Missing {
  if (dispute.reportHash !== null) return unavailable('body_missing', true)
  // The optimistic track has no panel yet; a challenge brings the dispute to
  // `Committing`, and the report with it.
  if (dispute.state === 'OptimisticPending') return { status: 'pending' }
  if (dispute.escalated) return unavailable('escalated', true)
  if (!needsReport(dispute, nowMs)) return unavailable('window_closed', true)
  if (transient !== null) return unavailable(transient, false)
  return { status: 'pending' }
}

/**
 * A base58 address of exactly 32 bytes. The length is checked on the decoded
 * bytes: `new PublicKey` accepts shorter strings and pads them, and `"1"`
 * would become the system program.
 */
const address = z.string().refine((value) => {
  try {
    return utils.bytes.bs58.decode(value).length === 32
  } catch {
    return false
  }
}, 'not a 32-byte base58 address')

/**
 * The contract checked on the stored value itself, without taking zod's copy:
 * the parsed copy drops unknown keys, and a key added in the database would
 * vanish from the hash while the body kept it. What is served and what is
 * hashed are the same object.
 */
const isReport = (value: unknown): value is FactFindingReport =>
  factFindingReport.safeParse(value).success

const fail = (
  c: Context,
  status: ContentfulStatusCode,
  code: ApiError['error']['code'],
  message: string,
) => c.json<ApiError>({ error: { code, message } }, status)

export function reportRoutes(options: ReportRoutesOptions): Hono {
  const { reports, disputes, chain, reporter, log } = options
  const now = options.now ?? Date.now
  const app = new Hono()

  app.get('/disputes/:pda/report', async (c) => {
    const pda = address.safeParse(c.req.param('pda'))
    if (!pda.success) return fail(c, 400, 'INVALID_INPUT', 'pda is not a 32-byte base58 address')

    // The database first: a dispute without a report is the common case, and
    // it should not cost an RPC call.
    const stored = await reports.first(pda.data)
    if (stored === null) {
      const dispute = await disputes.find(pda.data)
      if (dispute === null) return fail(c, 404, 'NOT_FOUND', 'No such dispute')
      return c.json<ReportResponse>(
        missingReportStatus(dispute, reporter.unavailable(pda.data), now()),
      )
    }

    if (!isReport(stored)) {
      log.error({ pda: pda.data }, 'stored report does not match the contract')
      return fail(c, 500, 'INTERNAL', 'The stored report is malformed')
    }

    let read: Awaited<ReturnType<typeof chain.readDispute>>
    try {
      read = await chain.readDispute(pda.data)
    } catch (err) {
      log.error({ err, pda: pda.data }, 'dispute account could not be read')
      return fail(
        c,
        503,
        'INTERNAL',
        'The chain could not be read; the report is not served unchecked',
      )
    }

    const hash = reportHash(stored)
    // No account — nothing on chain to match. No fingerprint yet — the same.
    const onchain = read ? disputeSnapshot(read.account, read.slot).reportHash : null

    return c.json<ReportResponse>({
      status: 'ready',
      report: stored,
      hash,
      matchesOnchain: onchain !== null && onchain === hash,
    })
  })

  return app
}
