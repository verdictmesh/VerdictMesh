import { PublicKey } from '@solana/web3.js'
import { apiError, disputeView } from '@verdictmesh/shared'
import { describe, expect, it, vi } from 'vitest'
import { z } from 'zod'
import {
  type DisputeFilter,
  type DisputeMirror,
  disputeRoutes,
  LIST_LIMIT,
  type MirrorDispute,
  type MirrorEntry,
} from './disputes.js'

const key = (fill: number) => new PublicKey(new Uint8Array(32).fill(fill)).toBase58()
const integrator = key(30)
const foreignIntegrator = key(31)

const mirrorDispute = (fill: number, overrides: Partial<MirrorDispute> = {}): MirrorDispute => ({
  pda: key(fill),
  integrator,
  escrowRef: key(2),
  claimant: key(3),
  respondent: key(4),
  amount: 18_446_744_073_709_551_615n,
  state: 'Finalized',
  panel: [key(5), key(6), key(7)],
  reportHash: null,
  claimantClaimHash: '1'.repeat(64),
  respondentClaimHash: '2'.repeat(64),
  openedAt: 1_700_000_000,
  commitDeadline: 1_700_000_060,
  revealDeadline: 1_700_000_120,
  appealDeadline: 1_700_000_180,
  votesClaimant: 2,
  votesRespondent: 1,
  escalated: false,
  verdict: 'Claimant',
  syncedSlot: 100,
  ...overrides,
})

const setup = (entries: MirrorEntry[]) => {
  const list = vi.fn(async (filter: DisputeFilter, limit: number) =>
    entries
      .filter((entry) => filter.pda === undefined || entry.dispute.pda === filter.pda)
      .slice(0, limit),
  )
  const mirror: DisputeMirror = { list }
  const settlements = {
    tracks: vi.fn(
      async (integrators: readonly string[]) =>
        new Map(integrators.map((address) => [address, address === integrator])),
    ),
    refresh: vi.fn(),
  }
  return { app: disputeRoutes({ mirror, settlements }), list, settlements }
}

const settled = { signature: '5'.repeat(88), slot: 42 }

describe('GET /disputes', () => {
  it('serves the contract, with u64 amounts whole', async () => {
    const t = setup([{ dispute: mirrorDispute(9), settlement: null }])
    const response = await t.app.request('/disputes')
    expect(response.status).toBe(200)

    const body = z.array(disputeView).parse(await response.json())
    expect(body[0]?.amount).toBe('18446744073709551615')
    expect(body[0]?.votesClaimant).toBe(2)
  })

  it('never leaks mirror-only columns', async () => {
    const t = setup([{ dispute: mirrorDispute(9), settlement: null }])
    const [first] = (await (await t.app.request('/disputes')).json()) as Record<string, unknown>[]
    expect(Object.keys(first ?? {})).not.toContain('syncedSlot')
    expect(Object.keys(first ?? {})).not.toContain('claimantClaimHash')
  })

  it('tells settled, awaiting and untracked apart', async () => {
    const t = setup([
      { dispute: mirrorDispute(9), settlement: settled },
      { dispute: mirrorDispute(10), settlement: null },
      { dispute: mirrorDispute(11, { integrator: foreignIntegrator }), settlement: null },
    ])
    const body = z.array(disputeView).parse(await (await t.app.request('/disputes')).json())
    expect(body.map((view) => view.settlement)).toEqual([
      { status: 'settled', ...settled },
      { status: 'awaiting' },
      { status: 'untracked' },
    ])
  })

  it('passes the filters through and caps the list', async () => {
    const t = setup([])
    const juror = key(5)
    await t.app.request(`/disputes?state=Committing&integrator=${integrator}&juror=${juror}`)
    expect(t.list).toHaveBeenCalledWith({ state: 'Committing', integrator, juror }, LIST_LIMIT)
  })

  it.each([
    ['an unknown state', '?state=Open'],
    ['a short juror address', '?juror=1'],
    ['a non-base58 integrator', `?integrator=${'0'.repeat(44)}`],
  ])('rejects %s with the error envelope', async (_, query) => {
    const t = setup([])
    const response = await t.app.request(`/disputes${query}`)
    expect(response.status).toBe(400)
    expect(apiError.parse(await response.json()).error.code).toBe('INVALID_INPUT')
    expect(t.list).not.toHaveBeenCalled()
  })
})

describe('GET /disputes/:pda', () => {
  it('serves one dispute', async () => {
    const t = setup([{ dispute: mirrorDispute(9), settlement: settled }])
    const response = await t.app.request(`/disputes/${key(9)}`)
    expect(response.status).toBe(200)
    expect(disputeView.parse(await response.json()).pda).toBe(key(9))
  })

  it('answers 404 for a dispute the mirror does not know', async () => {
    const t = setup([])
    const response = await t.app.request(`/disputes/${key(9)}`)
    expect(response.status).toBe(404)
    expect(apiError.parse(await response.json()).error.code).toBe('NOT_FOUND')
  })

  it('answers 400 for a malformed address', async () => {
    const t = setup([])
    expect((await t.app.request('/disputes/1')).status).toBe(400)
  })

  it('looks for the settlement again when a decided dispute is opened unsettled', async () => {
    const t = setup([{ dispute: mirrorDispute(9), settlement: null }])
    await t.app.request(`/disputes/${key(9)}`)
    expect(t.settlements.refresh).toHaveBeenCalledWith(expect.objectContaining({ pda: key(9) }))
  })

  it.each([
    ['already settled', { dispute: mirrorDispute(9), settlement: settled }],
    [
      'not decided yet',
      {
        dispute: mirrorDispute(9, { state: 'Committing', verdict: null, appealDeadline: 0 }),
        settlement: null,
      },
    ],
    [
      'settled by an escrow we cannot read',
      { dispute: mirrorDispute(9, { integrator: foreignIntegrator }), settlement: null },
    ],
  ])('does not look again when %s', async (_, entry) => {
    const t = setup([entry])
    await t.app.request(`/disputes/${key(9)}`)
    expect(t.settlements.refresh).not.toHaveBeenCalled()
  })
})
