import { BN, BorshCoder, type Idl } from '@coral-xyz/anchor'
import { PublicKey } from '@solana/web3.js'
import type { evidence } from '@verdictmesh/db'
import { parseLogs } from './logs.js'

/**
 * On-chain evidence for a dispute, collected by `escrow_ref` — `FR-015`,
 * `FR-016`.
 *
 * **The escrow is somebody else's program.** `escrow_ref` is an account owned
 * by whatever program the integrator registered, so the collector cannot
 * assume a layout. What it can always get is generic: the transactions that
 * touched the account (signers, block time, the programs they ran) and the
 * account itself (owner, size). Decoding events and the account is a bonus
 * that comes from `programs` — the IDLs we happen to have. `reference_escrow`
 * is one of them, not a special case.
 *
 * **`FR-016` knows exactly two sources of a fact — a transaction signature and
 * an account address — and the table is keyed by `(dispute_pda, source)`.** So
 * a transaction is one row however many events it carries: opening a dispute
 * emits `MilestoneDisputed` from the escrow and `DisputeOpened` from
 * VerdictMesh in the same transaction, and both go into that row's `events`.
 * The escrow account is one row too, holding the snapshot at the slot of the
 * read.
 *
 * **Nothing here is partial.** A transaction that fails to load fails the
 * whole collection: once in the table, an incomplete set looks exactly like a
 * complete one, and the model would call a fact unconfirmed only because its
 * transaction was not fetched. The one cut that is allowed — the cap on the
 * number of transactions — is reported in `truncated`, so the report can name
 * it among its gaps instead of staying silent about it.
 */

/** A row of the `evidence` table. The type comes from the table itself. */
export type EvidenceRow = typeof evidence.$inferInsert & { payload: Record<string, Json> }

export type Json = string | number | boolean | null | Json[] | { [key: string]: Json }

/** One entry of `getSignaturesForAddress`, reduced to what is used here. */
export interface SignatureInfo {
  signature: string
  slot: number
  /** The transaction failed and changed no state. */
  failed: boolean
}

export interface ChainTransaction {
  slot: number
  /** Unix seconds. `null` when the node has no estimate for that slot. */
  blockTime: number | null
  signers: readonly string[]
  /** `null` when the node did not keep the logs. */
  logs: readonly string[] | null
}

export interface EvidenceAccount {
  slot: number
  owner: string
  data: Uint8Array
}

/** The chain as the collector needs it. Everything else is the adapter's job. */
export interface EvidenceChain {
  /** The account and the slot it was read at. `null` — no account there. */
  readAccount(address: string): Promise<EvidenceAccount | null>
  /** Signatures of transactions that mention the address, newest first. */
  signaturesFor(address: string, limit: number): Promise<readonly SignatureInfo[]>
  /** `null` — the node does not have the transaction. */
  readTransaction(signature: string): Promise<ChainTransaction | null>
}

/**
 * A program whose IDL we have. `programId` is the address **in the network**,
 * not the one in the IDL: `anchor` re-keys a program on a fresh clone.
 */
export interface KnownProgram {
  name: string
  programId: PublicKey
  idl: Idl
}

export interface EvidenceTarget {
  /** The dispute the evidence belongs to. */
  pda: string
  /** `Dispute.escrow_ref` — the account whose history is the subject. */
  escrowRef: string
}

export interface CollectOptions {
  chain: EvidenceChain
  programs: readonly KnownProgram[]
  target: EvidenceTarget
  /**
   * How many successful transactions to read at most. The reference escrow
   * stays well under the default — one creation plus a release, a dispute and a
   * settlement for each of at most eight milestones is 25 — and `PLAN.md`
   * budgets about 30 RPC calls per dispute.
   */
  maxTransactions?: number
  /** How many transactions to read at once. */
  concurrency?: number
}

export interface EvidenceSet {
  rows: EvidenceRow[]
  /** Some history was left out: over the cap, or past one signature page. */
  truncated: boolean
}

const DEFAULT_MAX_TRANSACTIONS = 50
const DEFAULT_CONCURRENCY = 4

/**
 * The most `getSignaturesForAddress` returns in one call. One page is all the
 * collector asks for: the cap on transactions is far below it, and a history
 * longer than a page is exactly what `truncated` is for.
 */
const SIGNATURE_PAGE = 1_000

/**
 * Chain values as plain JSON for a `jsonb` column and for the model.
 *
 * Keys become base58, `BN` becomes a decimal string (a `u64` does not survive
 * `Number`), bytes become lowercase hex. Anything else that is not already JSON
 * is refused rather than guessed at: `JSON.stringify` would turn `NaN` into
 * `null` and a `Date` into a string, and neither would ever be noticed.
 */
export function toJson(value: unknown): Json {
  if (value === null || value === undefined) return null
  if (typeof value === 'string' || typeof value === 'boolean') return value
  if (typeof value === 'number') {
    if (!Number.isFinite(value)) throw new Error(`Not a finite number: ${value}`)
    return value
  }
  if (BN.isBN(value)) return value.toString()
  if (value instanceof PublicKey) return value.toBase58()
  if (value instanceof Uint8Array) return Buffer.from(value).toString('hex')
  if (Array.isArray(value)) return value.map(toJson)
  if (typeof value === 'object' && Object.getPrototypeOf(value) === Object.prototype) {
    return Object.fromEntries(Object.entries(value).map(([field, inner]) => [field, toJson(inner)]))
  }
  throw new Error(`Cannot store ${Object.prototype.toString.call(value)} as evidence`)
}

/** The known programs, keyed by their address in the network. */
interface Decoders {
  coders: ReadonlyMap<string, BorshCoder>
  programs: ReadonlyMap<string, KnownProgram>
}

const decodersOf = (known: readonly KnownProgram[]): Decoders => ({
  coders: new Map(
    known.map((program) => [program.programId.toBase58(), new BorshCoder(program.idl)]),
  ),
  programs: new Map(known.map((program) => [program.programId.toBase58(), program])),
})

function transactionRow(
  target: EvidenceTarget,
  signature: string,
  transaction: ChainTransaction,
  decoders: Decoders,
): EvidenceRow {
  if (transaction.logs === null) throw new Error(`Transaction ${signature} came without logs`)

  const parsed = parseLogs(transaction.logs, decoders.coders)
  const events = parsed.events.map((event) => ({
    program: decoders.programs.get(event.programId)?.name ?? event.programId,
    name: event.name,
    data: toJson(event.data),
  }))

  return {
    disputePda: target.pda,
    kind: 'transaction',
    source: signature,
    slot: transaction.slot,
    payload: {
      blockTime: transaction.blockTime,
      signers: [...transaction.signers],
      // Every program that ran: for an escrow we have no IDL for, this is how
      // the model tells a transaction that ran the escrow from one that merely
      // mentioned its account.
      programs: parsed.programs,
      events,
      // Events past the cut are gone for good, and the model has to know that
      // the silence of this transaction is not evidence of anything.
      logsTruncated: parsed.truncated,
    },
  }
}

function accountRow(
  target: EvidenceTarget,
  account: EvidenceAccount,
  decoders: Decoders,
): EvidenceRow {
  const data = Buffer.from(account.data)
  const program = decoders.programs.get(account.owner)
  const coder = decoders.coders.get(account.owner)

  const decoded = (() => {
    if (!program || !coder) return { program: null, account: null, data: null }

    const known = program.idl.accounts?.find(({ discriminator }) =>
      data.subarray(0, discriminator.length).equals(Buffer.from(discriminator)),
    )
    if (!known) {
      throw new Error(
        `Account ${target.escrowRef} of ${program.name} matches no account in its vendored IDL — the IDL and the deployed program have drifted apart`,
      )
    }

    return {
      program: program.name,
      account: known.name,
      data: toJson(coder.accounts.decode(known.name, data)),
    }
  })()

  return {
    disputePda: target.pda,
    kind: 'account',
    source: target.escrowRef,
    slot: account.slot,
    payload: { owner: account.owner, ...decoded, bytes: data.length },
  }
}

/** `map` with at most `limit` calls in flight; order of results is kept. */
async function mapLimited<In, Out>(
  items: readonly In[],
  limit: number,
  map: (item: In) => Promise<Out>,
): Promise<Out[]> {
  const results = new Array<Out>(items.length)
  let next = 0

  const worker = async () => {
    while (next < items.length) {
      const index = next
      next += 1
      // biome-ignore lint/style/noNonNullAssertion: index is below length.
      results[index] = await map(items[index]!)
    }
  }

  await Promise.all(Array.from({ length: Math.min(limit, items.length) }, worker))
  return results
}

export async function collectEvidence(options: CollectOptions): Promise<EvidenceSet> {
  const { chain, target } = options
  const maxTransactions = options.maxTransactions ?? DEFAULT_MAX_TRANSACTIONS
  const concurrency = options.concurrency ?? DEFAULT_CONCURRENCY
  const decoders = decodersOf(options.programs)

  const [account, signatures] = await Promise.all([
    chain.readAccount(target.escrowRef),
    chain.signaturesFor(target.escrowRef, SIGNATURE_PAGE),
  ])

  // A failed transaction changed nothing, and its logs may still hold events
  // that were emitted before the failure and then rolled back — decoding them
  // would hand the model facts that never happened.
  const successful = signatures.filter((entry) => !entry.failed)

  // Newest first, so the cut drops the oldest history. What the deal is about
  // survives it anyway: the account snapshot below carries the current terms.
  const kept = successful.slice(0, maxTransactions)
  const truncated = successful.length > kept.length || signatures.length >= SIGNATURE_PAGE

  const transactions = await mapLimited(kept, concurrency, async ({ signature }) => {
    const transaction = await chain.readTransaction(signature)
    if (!transaction) throw new Error(`Transaction ${signature} is not available from the node`)
    return transactionRow(target, signature, transaction, decoders)
  })

  const rows = account ? [...transactions, accountRow(target, account, decoders)] : transactions
  return { rows, truncated }
}
