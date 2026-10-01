import { BorshCoder } from '@coral-xyz/anchor'
import type { PublicKey } from '@solana/web3.js'
import type { EvidenceChain, KnownProgram } from './evidence.js'
import { parseLogs } from './logs.js'
import type { DisputeRow } from './watcher.js'

/**
 * Finding the escrow's settlement transaction of a dispute (`FR-020`).
 *
 * The verdict is pulled, not pushed: the escrow reads the `Dispute` account and
 * moves the funds itself, in a program that is not ours. So "settled" is never
 * something VerdictMesh can say of itself — it is read off the escrow's own
 * event, and only for an escrow whose IDL we hold. For any other escrow the
 * honest answer is `untracked`, not `awaiting`.
 *
 * Two ways in, and neither is enough alone:
 *
 * - **the escrow's logs, live.** A settlement shows up in seconds while the
 *   service is awake.
 * - **a lookup by the dispute's address.** The settling transaction reads the
 *   `Dispute` account, so it is among the signatures of that address. This is
 *   what catches a settlement that happened while the service slept — and on a
 *   free host it sleeps for hours.
 *
 * A found settlement is stored and never looked for again. A lookup that finds
 * nothing is not stored: it is repeated once per process for each decided
 * dispute, plus on demand when someone opens the dispute — so what the sleep
 * costs is one lookup per still-unsettled dispute per wake, not per sweep.
 */

/** An escrow program and the event in which it settles a dispute. */
export interface SettlingEscrow extends KnownProgram {
  /** An event of this program carrying the dispute's address in `dispute`. */
  settledEvent: string
}

export interface SettlementRow {
  disputePda: string
  escrowProgram: string
  signature: string
  slot: number
}

export interface SettlementStore {
  save(row: SettlementRow): Promise<void>
  /** Of the given disputes, the ones that already have a stored settlement. */
  settled(disputePdas: readonly string[]): Promise<ReadonlySet<string>>
}

export interface SettlementChain extends Pick<EvidenceChain, 'signaturesFor' | 'readTransaction'> {
  /** Logs of successful transactions of one program, with where they landed. */
  subscribeProgramLogs(
    programId: PublicKey,
    onLogs: (signature: string, slot: number, logs: readonly string[]) => void,
  ): Promise<() => Promise<void>>
  /**
   * The escrow program each `Integrator` account names, by the account's
   * address. `null` — no such account.
   */
  integratorEscrows(addresses: readonly string[]): Promise<ReadonlyMap<string, string | null>>
}

export interface SettlementLog {
  info(fields: Record<string, unknown>, message: string): void
  warn(fields: Record<string, unknown>, message: string): void
}

/** Exactly the part of a mirror row a settlement lookup needs. */
export type DecidedDispute = Pick<DisputeRow, 'pda' | 'integrator' | 'verdict'>

/**
 * Settlements in one transaction's logs. Only an event logged in the frame of
 * a settling escrow counts — `parseLogs` already refuses to decode anyone
 * else's bytes as ours.
 */
export function settlementsIn(
  logs: readonly string[],
  signature: string,
  slot: number,
  escrows: readonly SettlingEscrow[],
): SettlementRow[] {
  const coders = new Map(escrows.map((e) => [e.programId.toBase58(), new BorshCoder(e.idl)]))
  const events = new Map(escrows.map((e) => [e.programId.toBase58(), e.settledEvent]))

  return parseLogs(logs, coders).events.flatMap((event) => {
    if (events.get(event.programId) !== event.name) return []
    const dispute = event.data.dispute
    if (typeof dispute !== 'object' || dispute === null || !('toBase58' in dispute)) return []
    if (typeof dispute.toBase58 !== 'function') return []
    const disputePda: unknown = dispute.toBase58()
    if (typeof disputePda !== 'string') return []
    return [{ disputePda, escrowProgram: event.programId, signature, slot }]
  })
}

export interface SettlementsOptions {
  chain: SettlementChain
  store: SettlementStore
  escrows: readonly SettlingEscrow[]
  log: SettlementLog
  /** How many of the dispute's newest signatures a lookup reads. */
  lookupDepth?: number
  /** Milliseconds between two on-demand lookups of the same dispute. */
  refreshMs?: number
  /** Milliseconds. */
  now?: () => number
}

export interface Settlements {
  start(): Promise<void>
  stop(): Promise<void>
  /**
   * Whether the integrator's escrow is one we can read. Immutable on chain —
   * `register_integrator` is the only writer — so it is read once and kept.
   */
  tracks(integrators: readonly string[]): Promise<ReadonlyMap<string, boolean>>
  /** Decided disputes from a mirror snapshot: each is looked up once per process. */
  consider(rows: readonly DecidedDispute[]): void
  /** A dispute someone is looking at: looked up again, at most once per `refreshMs`. */
  refresh(row: DecidedDispute): void
  /** Resolves when nothing is queued or running. */
  idle(): Promise<void>
}

export function createSettlements(options: SettlementsOptions): Settlements {
  const { chain, store, escrows, log } = options
  const depth = options.lookupDepth ?? 25
  const refreshMs = options.refreshMs ?? 60_000
  const now = options.now ?? Date.now
  const escrowIds = new Set(escrows.map((e) => e.programId.toBase58()))

  const escrowOf = new Map<string, string | null>()
  const found = new Set<string>()
  const lastLookup = new Map<string, number>()
  const queue: string[] = []
  const queued = new Set<string>()
  const unsubscribe: (() => Promise<void>)[] = []
  const pending = new Set<Promise<void>>()
  let running: Promise<void> | null = null

  const save = async (row: SettlementRow) => {
    if (found.has(row.disputePda)) return
    await store.save(row)
    found.add(row.disputePda)
    log.info({ pda: row.disputePda, signature: row.signature }, 'settlement found')
  }

  const tracks = async (integrators: readonly string[]) => {
    const unknown = [...new Set(integrators)].filter((address) => !escrowOf.has(address))
    if (unknown.length > 0) {
      for (const [address, escrow] of await chain.integratorEscrows(unknown)) {
        escrowOf.set(address, escrow)
      }
    }
    return new Map(
      integrators.map((address) => {
        const escrow = escrowOf.get(address)
        return [address, escrow !== undefined && escrow !== null && escrowIds.has(escrow)]
      }),
    )
  }

  /** The dispute's newest signatures, newest first, until a settlement turns up. */
  const lookup = async (pda: string) => {
    lastLookup.set(pda, now())
    for (const entry of await chain.signaturesFor(pda, depth)) {
      if (entry.failed) continue
      const transaction = await chain.readTransaction(entry.signature)
      if (!transaction?.logs) continue
      const match = settlementsIn(
        transaction.logs,
        entry.signature,
        transaction.slot,
        escrows,
      ).find((row) => row.disputePda === pda)
      if (match) {
        await save(match)
        return
      }
    }
  }

  // One lookup at a time: a wake after a long sleep queues every unsettled
  // dispute at once, and the reporter shares the same RPC budget.
  const drain = async () => {
    while (queue.length > 0) {
      // biome-ignore lint/style/noNonNullAssertion: the queue is not empty.
      const pda = queue.shift()!
      queued.delete(pda)
      if (found.has(pda)) continue
      try {
        await lookup(pda)
      } catch (err) {
        log.warn({ err, pda }, 'settlement lookup failed')
      }
    }
  }

  const kick = () => {
    running ??= drain().finally(() => {
      running = null
    })
  }

  const enqueue = async (rows: readonly DecidedDispute[], due: (pda: string) => boolean) => {
    const decided = rows.filter((row) => row.verdict !== null && row.verdict !== undefined)
    if (decided.length === 0) return
    const stored = await store.settled(decided.map((row) => row.pda))
    for (const pda of stored) found.add(pda)
    const tracked = await tracks(decided.map((row) => row.integrator))
    for (const row of decided) {
      if (found.has(row.pda) || queued.has(row.pda) || !tracked.get(row.integrator)) continue
      if (!due(row.pda)) continue
      queue.push(row.pda)
      queued.add(row.pda)
    }
    kick()
  }

  const report = (err: unknown) => log.warn({ err }, 'settlement scan failed')

  const track = (work: Promise<void>) => {
    const settled = work.catch(report).finally(() => pending.delete(settled))
    pending.add(settled)
  }

  return {
    async start() {
      for (const escrow of escrows) {
        unsubscribe.push(
          await chain.subscribeProgramLogs(escrow.programId, (signature, slot, logs) => {
            for (const row of settlementsIn(logs, signature, slot, escrows)) {
              save(row).catch(report)
            }
          }),
        )
      }
    },

    async stop() {
      await Promise.all(unsubscribe.splice(0).map((off) => off()))
    },

    tracks,

    consider(rows) {
      track(enqueue(rows, (pda) => !lastLookup.has(pda)))
    },

    refresh(row) {
      track(enqueue([row], (pda) => now() - (lastLookup.get(pda) ?? -Infinity) >= refreshMs))
    },

    async idle() {
      while (pending.size > 0 || running) {
        await Promise.all([...pending])
        await running
      }
    },
  }
}
