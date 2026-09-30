import Anthropic from '@anthropic-ai/sdk'
import { serve } from '@hono/node-server'
import { Connection, PublicKey } from '@solana/web3.js'
import { createDb } from '@verdictmesh/db'
import type { ApiError } from '@verdictmesh/shared'
import { Hono } from 'hono'
import { pino } from 'pino'
import { reporterKeypair, solanaAttester } from './attest.js'
import {
  postgresCache,
  postgresEvidenceStore,
  postgresMirroredDisputes,
  postgresPublishedReports,
  postgresReportStore,
} from './cache.js'
import { solanaChain, solanaEvidenceChain } from './chain.js'
import { referenceEscrowPositions } from './claims.js'
import { loadEnv } from './env.js'
import { collectEvidence, type KnownProgram } from './evidence.js'
import { referenceEscrowIdl } from './idl/reference-escrow.js'
import { verdictMeshIdl } from './idl/verdict-mesh.js'
import { anthropicReportModel, createReporter } from './reporter.js'
import { reportRoutes } from './routes/reports.js'
import { createWatcher } from './watcher.js'

const env = loadEnv()
const log = pino({ level: env.LOG_LEVEL })

/**
 * HTTP and the watcher share one process and one database pool (`PLAN.md` →
 * "Free tier capacity"). Splitting them into two services on a free plan would
 * double the connections to a shared pgbouncer for the sake of two jobs that do
 * not load either of them.
 */
const connection = new Connection(env.SOLANA_RPC_URL, {
  commitment: 'confirmed',
  wsEndpoint: env.SOLANA_WS_URL,
})

const programId = new PublicKey(env.VERDICT_MESH_PROGRAM_ID)
const db = createDb(env.DATABASE_URL)

/** Addresses in this network, not the ones the vendored IDLs carry. */
const programs: KnownProgram[] = [
  { name: 'verdict_mesh', programId, idl: verdictMeshIdl },
  {
    name: 'reference_escrow',
    programId: new PublicKey(env.REFERENCE_ESCROW_PROGRAM_ID),
    idl: referenceEscrowIdl,
    positions: referenceEscrowPositions,
  },
]

const chain = solanaChain(connection, programId)
const evidenceChain = solanaEvidenceChain(connection)

const reporter = createReporter({
  collect: (target) => collectEvidence({ chain: evidenceChain, programs, target }),
  evidence: postgresEvidenceStore(db),
  reports: postgresReportStore(db),
  attester: solanaAttester(connection, programId, reporterKeypair(env.REPORTER_KEYPAIR)),
  // One retry, not the default two, and a timeout well under the default ten
  // minutes: past `SC-003` a report is late either way, and a hung request
  // would hold a slot of the queue that the next dispute needs.
  model: anthropicReportModel(
    new Anthropic({ apiKey: env.ANTHROPIC_API_KEY, maxRetries: 1, timeout: 90_000 }),
  ),
  programs,
  log,
})

const watcher = createWatcher({
  chain,
  cache: postgresCache(db),
  log,
  programId,
  onSnapshot: (rows) => reporter.consider(rows),
})

const app = new Hono()

app.get('/health', (c) => c.json({ ok: true }))
app.route(
  '/',
  reportRoutes({
    reports: postgresPublishedReports(db),
    disputes: postgresMirroredDisputes(db),
    chain,
    reporter,
    log,
  }),
)

// Hono answers an unhandled throw with a plain-text 500; the contract promises
// the error envelope on every failure.
app.onError((err, c) => {
  log.error({ err, path: c.req.path }, 'request failed')
  return c.json<ApiError>({ error: { code: 'INTERNAL', message: 'Internal error' } }, 500)
})

serve({ fetch: app.fetch, port: env.PORT })

// After `serve`, not before it: the first rewrite of the mirror can take
// seconds, and `/health` has to answer throughout — otherwise the host decides
// the service never came up and restarts it in the middle of that rewrite.
await watcher.start()

for (const signal of ['SIGINT', 'SIGTERM'] as const) {
  process.once(signal, () => {
    reporter.stop()
    void watcher.stop().finally(() => process.exit(0))
  })
}
