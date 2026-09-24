import { serve } from '@hono/node-server'
import { Connection, PublicKey } from '@solana/web3.js'
import { createDb } from '@verdictmesh/db'
import { Hono } from 'hono'
import { pino } from 'pino'
import { postgresCache } from './cache.js'
import { solanaChain } from './chain.js'
import { loadEnv } from './env.js'
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

const watcher = createWatcher({
  chain: solanaChain(connection, programId),
  cache: postgresCache(db),
  log,
  programId,
})

const app = new Hono()

app.get('/health', (c) => c.json({ ok: true }))

serve({ fetch: app.fetch, port: env.PORT })

// After `serve`, not before it: the first rewrite of the mirror can take
// seconds, and `/health` has to answer throughout — otherwise the host decides
// the service never came up and restarts it in the middle of that rewrite.
await watcher.start()

for (const signal of ['SIGINT', 'SIGTERM'] as const) {
  process.once(signal, () => {
    void watcher.stop().finally(() => process.exit(0))
  })
}
