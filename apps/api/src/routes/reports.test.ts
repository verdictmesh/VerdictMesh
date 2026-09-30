import { createHash } from 'node:crypto'
import { BN, BorshCoder } from '@coral-xyz/anchor'
import { PublicKey } from '@solana/web3.js'
import type { FactFindingReport } from '@verdictmesh/shared'
import { apiError, reportResponse } from '@verdictmesh/shared'
import { describe, expect, it, vi } from 'vitest'
import { verdictMeshIdl } from '../idl/verdict-mesh.js'
import { canonicalJson, reportHash } from '../reporter.js'
import type { Chain } from '../watcher.js'
import { type PublishedReports, reportRoutes } from './reports.js'

/**
 * The account comes out of the coder built from the IDL, for the same reason
 * as in `watcher.test.ts`: the layout is the build's, not the test's.
 */
const coder = new BorshCoder(verdictMeshIdl)
const key = (fill: number) => new PublicKey(new Uint8Array(32).fill(fill))
const pda = key(9).toBase58()

const report: FactFindingReport = {
  summary: 'The seller shipped; the buyer says the parcel never arrived.',
  timeline: [{ at: 1_700_000_000, what: 'Escrow funded', sourceAccount: key(2).toBase58() }],
  facts: [
    {
      statement: 'The escrow holds 5 USDC',
      verdict: 'confirmed',
      sourceAccount: key(2).toBase58(),
    },
  ],
  claims: [{ party: 'claimant', statement: 'Nothing arrived', assessment: 'unsupported' }],
  gaps: ['No delivery record on chain'],
}

const fingerprint = (hex: string | null) =>
  hex === null ? Array<number>(32).fill(0) : [...Buffer.from(hex, 'hex')]

const disputeAccount = async (reportHashHex: string | null) => ({
  address: pda,
  data: await coder.accounts.encode('Dispute', {
    integrator: key(1),
    dispute_id: new BN(7),
    policy: {
      panel_size: 3,
      extended_panel_size: 5,
      quorum: 2,
      extended_quorum: 3,
      juror_stake: new BN(1_000),
      slash_bps_wrong: 1_000,
      slash_bps_no_reveal: 2_000,
      commit_window: new BN(60),
      reveal_window: new BN(60),
      appeal_window: new BN(60),
      optimistic_window: new BN(60),
      deposit: new BN(50),
      optimistic_threshold: new BN(0),
    },
    escrow_ref: key(2),
    claimant: key(3),
    respondent: key(4),
    amount: new BN(5_000_000),
    state: { Committing: {} },
    panel: [key(5), key(6), key(7)],
    report_hash: fingerprint(reportHashHex),
    claimant_claim_hash: Array<number>(32).fill(1),
    respondent_claim_hash: Array<number>(32).fill(2),
    opened_at: new BN(1_700_000_000),
    entropy_slot: new BN(12_345),
    commit_deadline: new BN(1_700_000_060),
    reveal_deadline: new BN(1_700_000_120),
    appeal_deadline: new BN(0),
    votes_claimant: 0,
    votes_respondent: 0,
    escalated: false,
    verdict: null,
    bump: 254,
  }),
})

const setup = ({
  stored = report as unknown,
  onchain = reportHash(report) as string | null,
  readDispute,
}: {
  stored?: unknown
  onchain?: string | null
  readDispute?: Chain['readDispute']
} = {}) => {
  const reports: PublishedReports = { first: vi.fn(async () => stored) }
  const chain = {
    readDispute: vi.fn(
      readDispute ?? (async () => ({ slot: 500, account: await disputeAccount(onchain) })),
    ),
  }
  const log = { error: vi.fn() }
  const app = reportRoutes({ reports, chain, log })

  return { app, reports, chain, log }
}

const get = async (app: ReturnType<typeof setup>['app'], address = pda) => {
  const response = await app.request(`/disputes/${address}/report`)
  return { status: response.status, body: (await response.json()) as unknown }
}

describe('GET /disputes/:pda/report', () => {
  it('serves the report with its hash and a match when the chain holds that hash', async () => {
    const { app, reports, chain } = setup()

    const { status, body } = await get(app)

    expect(status).toBe(200)
    expect(reportResponse.parse(body)).toEqual({
      report,
      hash: reportHash(report),
      matchesOnchain: true,
    })
    expect(reports.first).toHaveBeenCalledWith(pda)
    expect(chain.readDispute).toHaveBeenCalledWith(pda)
  })

  /** `FR-017b` from the client's side: no trust in our hash is needed. */
  it('returns a hash any client reproduces from the served body alone', async () => {
    const { app } = setup()

    const { body } = await get(app)
    const served = reportResponse.parse(body)

    expect(createHash('sha256').update(canonicalJson(served.report), 'utf8').digest('hex')).toBe(
      served.hash,
    )
  })

  /** The substitution this route exists to expose: the body edited in the database. */
  it('reports a mismatch when the stored body is not the one fingerprinted on chain', async () => {
    const tampered = { ...report, summary: 'The seller never shipped.' }
    const { app } = setup({ stored: tampered })

    const { status, body } = await get(app)

    expect(status).toBe(200)
    expect(reportResponse.parse(body)).toEqual({
      report: tampered,
      hash: reportHash(tampered),
      matchesOnchain: false,
    })
  })

  /**
   * zod drops unknown keys from its copy. Hashing that copy would make the
   * addition invisible while serving it; the served object is what is hashed.
   */
  it('hashes a key added in the database instead of dropping it', async () => {
    const extended = { ...report, verdictHint: 'claimant' }
    const { app } = setup({ stored: extended })

    const { body } = await get(app)

    expect(body).toMatchObject({ report: extended, matchesOnchain: false })
    expect(body).toMatchObject({ hash: reportHash(extended) })
    expect(reportHash(extended)).not.toBe(reportHash(report))
  })

  /** Key order is not content: jsonb returns members in its own order. */
  it('matches regardless of the order the database returns the members in', async () => {
    const reordered = Object.fromEntries(Object.entries(report).reverse())
    const { app } = setup({ stored: reordered })

    expect((await get(app)).body).toMatchObject({ matchesOnchain: true })
  })

  it('does not match while no fingerprint is on chain yet', async () => {
    const { app } = setup({ onchain: null })

    expect((await get(app)).body).toMatchObject({ hash: reportHash(report), matchesOnchain: false })
  })

  it('does not match when the dispute account does not exist', async () => {
    const { app } = setup({ readDispute: async () => null })

    expect((await get(app)).body).toMatchObject({ matchesOnchain: false })
  })

  /** A report without the check is the read path RLS closes (T026). */
  it('serves nothing when the chain cannot be read', async () => {
    const { app, log } = setup({
      readDispute: async () => {
        throw new Error('429 Too Many Requests')
      },
    })

    const { status, body } = await get(app)

    expect(status).toBe(503)
    expect(apiError.parse(body).error.code).toBe('INTERNAL')
    expect(body).not.toHaveProperty('report')
    expect(log.error).toHaveBeenCalledOnce()
  })

  it('answers 404 without an RPC call when there is no report', async () => {
    const { app, chain } = setup({ stored: null })

    const { status, body } = await get(app)

    expect(status).toBe(404)
    expect(apiError.parse(body).error.code).toBe('NOT_FOUND')
    expect(chain.readDispute).not.toHaveBeenCalled()
  })

  it('refuses a stored body that breaks the contract instead of serving it', async () => {
    const { app, chain, log } = setup({ stored: { summary: 'no arrays' } })

    const { status, body } = await get(app)

    expect(status).toBe(500)
    expect(apiError.parse(body).error.code).toBe('INTERNAL')
    expect(chain.readDispute).not.toHaveBeenCalled()
    expect(log.error).toHaveBeenCalledOnce()
  })

  it('refuses anything that is not a 32-byte base58 address before any read', async () => {
    // `1` is what `new PublicKey` would quietly pad into the system program;
    // `0` and `l` are not in the base58 alphabet at all.
    for (const bad of ['1', 'abc', `${pda}1`, '0'.repeat(44), 'l'.repeat(44)]) {
      const { app, reports } = setup()

      const { status, body } = await get(app, bad)

      expect(status).toBe(400)
      expect(apiError.parse(body).error.code).toBe('INVALID_INPUT')
      expect(reports.first).not.toHaveBeenCalled()
    }
  })
})
