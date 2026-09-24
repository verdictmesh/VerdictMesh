import { BN, BorshCoder, EventParser } from '@coral-xyz/anchor'
import type { PublicKey } from '@solana/web3.js'
import type { disputes } from '@verdictmesh/db'
import { disputeState, verdict as verdictContract } from '@verdictmesh/shared'
import { z } from 'zod'
import { verdictMeshIdl } from './idl/verdict-mesh.js'

/**
 * A Postgres mirror of on-chain dispute state — `FR-029`.
 *
 * **Subscribing to logs is not enough on its own, and that is not a matter of
 * optimisation.** Events carry what happened; the table holds what a dispute
 * **is**. Deadlines are derived from the policy snapshot stored in the account,
 * and `DisputeOpened` does not carry them (`docs/TASKS.md` → T026). So an event
 * here is never a write — it is a **reason to read the account**. What lands in
 * the table is a snapshot, not a retelling of an event, and the slot of that
 * read lands in `synced_slot` so an older mirror can never overwrite a newer
 * one.
 *
 * **Recovery is a full rewrite, not a cursor.** After a restart the watcher
 * does not chase the events it missed. It reads every `Dispute` account of the
 * program with one `getProgramAccounts` and stores them as a snapshot taken at
 * a single slot. That costs more than walking signatures only while there are
 * fewer disputes than there were transactions during the downtime — never, on
 * this project — and in exchange:
 *
 * - **nothing can be lost.** Transaction logs are truncated at 10 KB and
 *   `PanelSelected` carries up to 32 keys; an event that did not fit would stay
 *   lost forever to a signature walk, while a rewrite never notices it was
 *   missing.
 * - **the cost does not grow with the downtime.** Free hosting sleeps for
 *   hours, and a signature walk after every wake-up would grow along with it.
 * - **no second source of truth about "where we stopped" appears.** That role
 *   belongs to `synced_slot`, which is needed as a staleness guard anyway.
 *
 * The same rewrite runs on a schedule: while the service is up, it is also what
 * heals a dropped WebSocket subscription and a truncated log.
 */

/** Decodes both accounts and events: one IDL, so the two cannot disagree. */
const coder = new BorshCoder(verdictMeshIdl)

/**
 * A `[u8; 32]` of zeroes. On chain that means "no report fingerprint yet"
 * (`FR-017`); in the database it means `null`. A column holding sixty-four
 * zeroes would read as a hash that simply matches nothing.
 */
const NO_HASH = '0'.repeat(64)

const bn = z.custom<BN>((value) => BN.isBN(value), { message: 'expected a BN' })

/**
 * A whole `u64`. It fits `numeric(20, 0)` exactly, while `Number` would drop
 * the low digits of the largest dispute — that is, where it costs the most.
 */
const u64 = bn.transform((value) => BigInt(value.toString()))

/**
 * A slot, or a unix timestamp in seconds. Both are `number` in `DisputeView`,
 * and the safe-integer bound is checked explicitly: `BN.toNumber` does not
 * throw straight away on a larger value, it silently returns an imprecise one,
 * and a deadline off by a few seconds would go into the database.
 */
const chainTime = bn
  .refine((value) => value.abs().bitLength() <= 53, {
    message: 'does not fit a safe integer',
  })
  .transform((value) => value.toNumber())

const pubkey = z
  .custom<PublicKey>(
    (value) => typeof value === 'object' && value !== null && 'toBase58' in value,
    { message: 'expected a public key' },
  )
  .transform((key) => key.toBase58())

/** `[u8; 32]` → lowercase hex: exactly what the `char(64)` column expects. */
const hash = z
  .array(z.number().int().min(0).max(255))
  .length(32)
  .transform((bytes) => Buffer.from(bytes).toString('hex'))

/**
 * Anchor returns an enum as a single-key object — `{ Revealing: {} }`. In the
 * raw IDL variant names stay in PascalCase, which is word for word what the
 * database enums and `packages/shared` hold, so nothing is translated between
 * the layers. The list comes from the contract on purpose: a new variant added
 * on chain becomes a parse error rather than a silent pass-through.
 */
const variantOf = <Names extends z.ZodType>(names: Names) =>
  z.preprocess(
    (value) =>
      typeof value === 'object' && value !== null
        ? // Joined, not `[0]`: an object with two keys has to fail against the
          // list instead of quietly yielding the first variant.
          Object.keys(value).join('+')
        : value,
    names,
  )

/**
 * The surface of the `Dispute` account as the mirror sees it. Field names are
 * snake_case because that is how `anchor build` leaves them and how
 * `BorshCoder` hands them back.
 *
 * `policy`, `dispute_id`, `entropy_slot` and `bump` are absent on purpose: the
 * policy snapshot is for the program rather than for a reader, the identity of
 * a dispute is its address, and the selection check (`FR-006a`) is read from
 * the chain on demand. Zod drops them silently — but a field **renamed on
 * chain** becomes a parse error instead of a zero in the database.
 */
export const disputeAccount = z.object({
  integrator: pubkey,
  escrow_ref: pubkey,
  claimant: pubkey,
  respondent: pubkey,
  amount: u64,
  state: variantOf(disputeState),
  panel: z.array(pubkey),
  report_hash: hash,
  claimant_claim_hash: hash,
  respondent_claim_hash: hash,
  opened_at: chainTime,
  commit_deadline: chainTime,
  reveal_deadline: chainTime,
  appeal_deadline: chainTime,
  votes_claimant: z.number().int().min(0),
  votes_respondent: z.number().int().min(0),
  escalated: z.boolean(),
  verdict: variantOf(verdictContract).nullable(),
})

/** A mirror row. The type comes from the table, so it cannot drift from it. */
export type DisputeRow = typeof disputes.$inferInsert

/** An account as RPC hands it over: an address and bytes, uninterpreted. */
export interface ChainAccount {
  address: string
  data: Uint8Array
}

/**
 * A `Dispute` account snapshot turned into a mirror row.
 *
 * `slot` is the slot of the **read**, not the slot of the event that prompted
 * it. That is the point: a mirror shows state rather than history, and a
 * snapshot taken after the event is fresher, not wrong.
 */
export function disputeSnapshot(account: ChainAccount, slot: number): DisputeRow {
  const decoded: unknown = coder.accounts.decode('Dispute', Buffer.from(account.data))
  const dispute = disputeAccount.parse(decoded)

  return {
    pda: account.address,
    integrator: dispute.integrator,
    escrowRef: dispute.escrow_ref,
    claimant: dispute.claimant,
    respondent: dispute.respondent,
    amount: dispute.amount,
    state: dispute.state,
    panel: dispute.panel,
    reportHash: dispute.report_hash === NO_HASH ? null : dispute.report_hash,
    claimantClaimHash: dispute.claimant_claim_hash,
    respondentClaimHash: dispute.respondent_claim_hash,
    openedAt: dispute.opened_at,
    commitDeadline: dispute.commit_deadline,
    revealDeadline: dispute.reveal_deadline,
    appealDeadline: dispute.appeal_deadline,
    votesClaimant: dispute.votes_claimant,
    votesRespondent: dispute.votes_respondent,
    escalated: dispute.escalated,
    verdict: dispute.verdict,
    syncedSlot: slot,
  }
}

/**
 * The addresses of the disputes mentioned in the logs of one transaction,
 * without repeats.
 *
 * Parsing goes through `EventParser` rather than scanning for `Program data:`
 * lines, because the parser follows the call stack: an event emitted by
 * **another** program in the same transaction never gets here. The `dispute`
 * field is not taken on faith either — `JurorStaked` and `JurorUnstaked` do not
 * have one at all, they are about the registry.
 */
export function disputesInLogs(parser: EventParser, logs: readonly string[]): string[] {
  const seen = new Set<string>()

  for (const event of parser.parseLogs([...logs])) {
    const dispute: unknown = event.data.dispute
    const parsed = pubkey.safeParse(dispute)
    if (parsed.success) seen.add(parsed.data)
  }

  return [...seen]
}

/**
 * The freshest row per dispute.
 *
 * Not an economy measure: a single `INSERT ... ON CONFLICT` cannot touch the
 * same row twice, and Postgres answers that with an error rather than with the
 * last value. A rewrite that read a dispute and received the same dispute from
 * the subscription while writing would assemble exactly such a batch.
 */
export function latestPerDispute(rows: readonly DisputeRow[]): DisputeRow[] {
  const latest = new Map<string, DisputeRow>()

  for (const row of rows) {
    const known = latest.get(row.pda)
    if (!known || known.syncedSlot <= row.syncedSlot) latest.set(row.pda, row)
  }

  return [...latest.values()]
}

/** The chain as the mirror needs it. Everything else is the adapter's job. */
export interface Chain {
  /** Every `Dispute` account of the program and **one** slot for all of them. */
  allDisputes(): Promise<{ slot: number; accounts: readonly ChainAccount[] }>
  /** An account and the slot it was read at. `null` — no account at that slot. */
  readDispute(address: string): Promise<{ slot: number; account: ChainAccount } | null>
  /** Live transaction logs of the program. Returns an unsubscribe handle. */
  subscribeLogs(onLogs: (logs: readonly string[]) => void): Promise<() => Promise<void>>
}

/** The mirror as the watcher needs it. Writes go in batches, not rows. */
export interface Cache {
  save(rows: readonly DisputeRow[]): Promise<void>
}

/** Exactly the part of pino the watcher uses. */
export interface WatcherLog {
  info(fields: Record<string, unknown>, message: string): void
  warn(fields: Record<string, unknown>, message: string): void
  error(fields: Record<string, unknown>, message: string): void
}

export interface WatcherOptions {
  chain: Chain
  cache: Cache
  log: WatcherLog
  /**
   * The address of the program **in the network**, not the one from the IDL:
   * `anchor` re-keys a program on a fresh clone, and the vendored IDL may well
   * carry a different address. Decoding does not care — the layout is the same
   * — but the visibility scope of events is set by this key.
   */
  programId: PublicKey
  /**
   * How often to rewrite the mirror in full. Zero disables it, which leaves the
   * subscription as the only source and turns a truncated transaction log into
   * a hole that lasts until the next restart.
   */
  resyncIntervalMs?: number
}

export interface Watcher {
  /**
   * Rewrite, then subscribe.
   *
   * A failed first rewrite does not abort the start: the mirror is a cache, the
   * source of truth stays on chain, and a service that refused to come up over
   * a one-minute RPC hiccup costs more than a service whose mirror is five
   * minutes stale. The error goes to the log and the next scheduled rewrite
   * heals it.
   */
  start(): Promise<void>
  stop(): Promise<void>
  /** Rewrite the whole mirror. Returns how many disputes were stored. */
  sweep(): Promise<number>
  /** Re-read the named disputes. Returns how many were stored. */
  refresh(addresses: readonly string[]): Promise<number>
}

/** Five minutes: several times less than an hour of free-tier sleep. */
const DEFAULT_RESYNC_MS = 5 * 60 * 1000

export function createWatcher(options: WatcherOptions): Watcher {
  const { chain, cache, log, programId } = options
  const resyncIntervalMs = options.resyncIntervalMs ?? DEFAULT_RESYNC_MS
  const parser = new EventParser(programId, coder)

  let unsubscribe: (() => Promise<void>) | null = null
  let resync: ReturnType<typeof setInterval> | null = null

  const sweep = async (): Promise<number> => {
    const { slot, accounts } = await chain.allDisputes()
    const rows = accounts.map((account) => disputeSnapshot(account, slot))
    await cache.save(rows)
    log.info({ disputes: rows.length, slot }, 'mirror rewritten from chain')
    return rows.length
  }

  const refresh = async (addresses: readonly string[]): Promise<number> => {
    const snapshots = await Promise.all(
      addresses.map(async (address): Promise<DisputeRow | null> => {
        try {
          const read = await chain.readDispute(address)
          // No instruction ever closes a dispute account, so nothing here means
          // either RPC lagging behind the event we have just seen, or reading
          // the wrong network. Losing it silently would look exactly like the
          // dispute not existing on chain, so it goes to the log; the next
          // rewrite brings it back.
          if (!read) {
            log.warn({ dispute: address }, 'dispute account not found')
            return null
          }
          return disputeSnapshot(read.account, read.slot)
        } catch (error: unknown) {
          // One at a time rather than all together: a single transaction
          // touches several disputes, and a `Promise.all` that rejects as a
          // whole would throw away two good reads along with the failed one.
          log.error({ err: error, dispute: address }, 'failed to read dispute account')
          return null
        }
      }),
    )

    const rows = snapshots.filter((row) => row !== null)
    if (rows.length > 0) await cache.save(rows)
    return rows.length
  }

  const onLogs = (logs: readonly string[]): void => {
    const addresses = disputesInLogs(parser, logs)
    if (addresses.length === 0) return

    // This handler runs inside a WebSocket callback: an error thrown out of it
    // has nowhere to go but `unhandledRejection`, which takes down the HTTP
    // layer along with the watcher. The periodic rewrite exists precisely so
    // that an event swallowed here does not stay swallowed forever.
    refresh(addresses).catch((error: unknown) => {
      log.error({ err: error, disputes: addresses }, 'failed to mirror disputes from logs')
    })
  }

  return {
    sweep,
    refresh,

    async start() {
      await sweep().catch((error: unknown) => {
        log.error({ err: error }, 'initial sweep failed, mirror starts stale')
      })
      unsubscribe = await chain.subscribeLogs(onLogs)

      if (resyncIntervalMs > 0) {
        resync = setInterval(() => {
          sweep().catch((error: unknown) => {
            log.error({ err: error }, 'periodic resync failed')
          })
        }, resyncIntervalMs)
        // A scheduled rewrite is no reason to keep the process alive.
        resync.unref?.()
      }

      log.info({ programId: programId.toBase58(), resyncIntervalMs }, 'watcher started')
    },

    async stop() {
      if (resync) clearInterval(resync)
      resync = null

      const stopSubscription = unsubscribe
      unsubscribe = null
      if (stopSubscription) await stopSubscription()
    },
  }
}
