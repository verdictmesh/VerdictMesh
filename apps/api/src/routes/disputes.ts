import type { disputes } from '@verdictmesh/db'
import type { DisputeState, DisputeView, Settlement } from '@verdictmesh/shared'
import { disputeState } from '@verdictmesh/shared'
import { Hono } from 'hono'
import { z } from 'zod'
import type { Settlements } from '../settlement.js'
import { address, fail } from './params.js'

/**
 * `GET /disputes` and `GET /disputes/:pda` — the dispute as anyone may see it
 * (`FR-020`, `FR-028a`).
 *
 * No wallet, no key, no account: the mirror holds nothing that is not already
 * public on chain, so reading it asks nothing of the reader. The panel's own
 * view of a juror (`FR-019`) is the same list filtered by `juror` — a filter,
 * not a permission: the right to vote is checked by the program at commit
 * time, never here (`FR-028`).
 *
 * Answered from the mirror alone. `SC-010` gives the first screen two seconds,
 * and an RPC call per row would spend them. The one RPC-backed part, whether
 * the integrator's escrow is one we can read, is read once per integrator per
 * process and kept: `register_integrator` is its only writer.
 */

export type MirrorDispute = typeof disputes.$inferSelect

export interface DisputeFilter {
  state?: DisputeState | undefined
  integrator?: string | undefined
  juror?: string | undefined
  pda?: string | undefined
}

export interface MirrorEntry {
  dispute: MirrorDispute
  settlement: { signature: string; slot: number } | null
}

export interface DisputeMirror {
  /** Newest first. */
  list(filter: DisputeFilter, limit: number): Promise<MirrorEntry[]>
}

export interface DisputeRoutesOptions {
  mirror: DisputeMirror
  settlements: Pick<Settlements, 'tracks' | 'refresh'>
}

/** More than any panel's backlog; the list is a screen, not an export. */
export const LIST_LIMIT = 100

const listQuery = z.object({
  state: disputeState.optional(),
  integrator: address.optional(),
  juror: address.optional(),
})

export function settlementOf(entry: MirrorEntry, tracked: boolean): Settlement {
  if (entry.settlement) return { status: 'settled', ...entry.settlement }
  return tracked ? { status: 'awaiting' } : { status: 'untracked' }
}

export function disputeViewOf(entry: MirrorEntry, tracked: boolean): DisputeView {
  const { dispute } = entry
  return {
    pda: dispute.pda,
    integrator: dispute.integrator,
    escrowRef: dispute.escrowRef,
    claimant: dispute.claimant,
    respondent: dispute.respondent,
    // A string, so a `u64` survives JSON whole.
    amount: dispute.amount.toString(),
    state: dispute.state,
    panel: dispute.panel,
    reportHash: dispute.reportHash,
    openedAt: dispute.openedAt,
    commitDeadline: dispute.commitDeadline,
    revealDeadline: dispute.revealDeadline,
    appealDeadline: dispute.appealDeadline,
    votesClaimant: dispute.votesClaimant,
    votesRespondent: dispute.votesRespondent,
    escalated: dispute.escalated,
    verdict: dispute.verdict,
    settlement: settlementOf(entry, tracked),
  }
}

export function disputeRoutes(options: DisputeRoutesOptions): Hono {
  const { mirror, settlements } = options
  const app = new Hono()

  const views = async (entries: readonly MirrorEntry[]) => {
    const tracked = await settlements.tracks(entries.map((entry) => entry.dispute.integrator))
    return entries.map((entry) =>
      disputeViewOf(entry, tracked.get(entry.dispute.integrator) ?? false),
    )
  }

  app.get('/disputes', async (c) => {
    const query = listQuery.safeParse(c.req.query())
    if (!query.success) {
      return fail(c, 400, 'INVALID_INPUT', 'state, integrator or juror is malformed')
    }
    return c.json<DisputeView[]>(await views(await mirror.list(query.data, LIST_LIMIT)))
  })

  app.get('/disputes/:pda', async (c) => {
    const pda = address.safeParse(c.req.param('pda'))
    if (!pda.success) return fail(c, 400, 'INVALID_INPUT', 'pda is not a 32-byte base58 address')

    const [entry] = await mirror.list({ pda: pda.data }, 1)
    if (!entry) return fail(c, 404, 'NOT_FOUND', 'No such dispute')

    const [view] = await views([entry])
    if (!view) return fail(c, 404, 'NOT_FOUND', 'No such dispute')

    // Someone is looking at a decided dispute we have not seen settled: look
    // again, in the background. The answer goes to the next request, not this
    // one — a lookup is a dozen RPC calls, and this screen is the party's.
    if (view.verdict !== null && view.settlement.status === 'awaiting') {
      settlements.refresh(entry.dispute)
    }

    return c.json<DisputeView>(view)
  })

  return app
}
