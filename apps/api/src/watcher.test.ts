import { BN, BorshCoder, EventParser } from '@coral-xyz/anchor'
import { PublicKey } from '@solana/web3.js'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { verdictMeshIdl } from './idl/verdict-mesh.js'
import {
  type Chain,
  type ChainAccount,
  createWatcher,
  type DisputeRow,
  disputeAccount,
  disputeSnapshot,
  disputesInLogs,
  latestPerDispute,
  type WatcherLog,
} from './watcher.js'

/**
 * The bytes for these tests are produced by the **same** `BorshCoder` that then
 * decodes them. That is not the coder checking itself: what is under test is
 * the translation of a snapshot into a mirror row, while the layout is dictated
 * by the IDL from the build — assembling it by hand would write our own guess
 * about that layout into the test (see `docs/TASKS.md`, T027).
 */
const coder = new BorshCoder(verdictMeshIdl)
const programId = new PublicKey(verdictMeshIdl.address)

const key = (fill: number) => new PublicKey(new Uint8Array(32).fill(fill))

const policy = {
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
}

/**
 * A dispute in its reveal window: the panel is selected, there is no report
 * yet and no verdict either.
 *
 * The type is deliberately loose: this is input for the encoder rather than a
 * value of our own contract, and narrowing it would mean describing the account
 * layout a second time — in the test.
 */
const revealing: Record<string, unknown> = {
  integrator: key(1),
  dispute_id: new BN(7),
  policy,
  escrow_ref: key(2),
  claimant: key(3),
  respondent: key(4),
  amount: new BN('18446744073709551615'),
  state: { Revealing: {} },
  panel: [key(5), key(6)],
  report_hash: Array<number>(32).fill(0),
  claimant_claim_hash: Array.from({ length: 32 }, (_, index) => index),
  respondent_claim_hash: Array<number>(32).fill(255),
  opened_at: new BN(1_700_000_000),
  entropy_slot: new BN(12_345),
  commit_deadline: new BN(1_700_000_060),
  reveal_deadline: new BN(1_700_000_120),
  appeal_deadline: new BN(0),
  votes_claimant: 1,
  votes_respondent: 0,
  escalated: false,
  verdict: null,
  bump: 254,
}

const encodeDispute = async (overrides: Record<string, unknown> = {}): Promise<ChainAccount> => ({
  address: key(9).toBase58(),
  data: await coder.accounts.encode('Dispute', { ...revealing, ...overrides }),
})

const encodeEvent = (name: string, data: Record<string, unknown>): string => {
  const event = verdictMeshIdl.events.find((candidate) => candidate.name === name)
  if (!event) throw new Error(`No such event in the IDL: ${name}`)
  return Buffer.concat([Buffer.from(event.discriminator), coder.types.encode(name, data)]).toString(
    'base64',
  )
}

const invocation = (program: PublicKey, ...events: string[]) => [
  `Program ${program.toBase58()} invoke [1]`,
  ...events.map((event) => `Program data: ${event}`),
  `Program ${program.toBase58()} success`,
]

describe('an account snapshot as a mirror row', () => {
  it('carries over every field the panel reads', async () => {
    const row = disputeSnapshot(await encodeDispute(), 4_242)

    expect(row).toEqual({
      pda: key(9).toBase58(),
      integrator: key(1).toBase58(),
      escrowRef: key(2).toBase58(),
      claimant: key(3).toBase58(),
      respondent: key(4).toBase58(),
      amount: 18_446_744_073_709_551_615n,
      state: 'Revealing',
      panel: [key(5).toBase58(), key(6).toBase58()],
      reportHash: null,
      claimantClaimHash: '000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f',
      respondentClaimHash: 'f'.repeat(64),
      openedAt: 1_700_000_000,
      commitDeadline: 1_700_000_060,
      revealDeadline: 1_700_000_120,
      appealDeadline: 0,
      votesClaimant: 1,
      votesRespondent: 0,
      escalated: false,
      verdict: null,
      syncedSlot: 4_242,
    })
  })

  /**
   * `u64::MAX` is not exotica for its own sake: `amount` is a `u64` on chain,
   * and a trip through `number` would drop its low digits in silence.
   */
  it('keeps every digit of the largest amount', async () => {
    const row = disputeSnapshot(await encodeDispute(), 1)
    expect(row.amount).toBe(2n ** 64n - 1n)
  })

  it('stores the slot of the read, not the slot of an event', async () => {
    expect(disputeSnapshot(await encodeDispute(), 777).syncedSlot).toBe(777)
  })

  it('gives the report fingerprint as hex once it stops being zeroes', async () => {
    const row = disputeSnapshot(
      await encodeDispute({ report_hash: Array<number>(32).fill(171) }),
      1,
    )
    expect(row.reportHash).toBe('ab'.repeat(32))
  })

  it('spells state and verdict exactly as the contract does', async () => {
    const row = disputeSnapshot(
      await encodeDispute({ state: { Finalized: {} }, verdict: { StatusQuo: {} } }),
      1,
    )
    expect(row.state).toBe('Finalized')
    expect(row.verdict).toBe('StatusQuo')
  })

  /**
   * Another account of the same program (`Juror`) has to hit the discriminator
   * rather than decode into something plausible: the layout of `Dispute` starts
   * with a `Pubkey`, and the first 32 bytes of any account look like one.
   */
  it('refuses to decode a different account', async () => {
    const juror = await coder.accounts.encode('Juror', {
      wallet: key(3),
      stake: new BN(10),
      active_disputes: 0,
      index: 0,
      bump: 255,
    })

    expect(() => disputeSnapshot({ address: key(3).toBase58(), data: juror }, 1)).toThrow()
  })
})

describe('the account surface as a boundary', () => {
  /**
   * A field renamed on chain has to become a parse error. Silently it would
   * become a zero in the database: `BorshCoder` decodes by layout rather than
   * by name, and the `escrow_ref` column would simply stay empty.
   */
  it('fails when a field has gone from the account', () => {
    const { escrow_ref: _renamed, ...withoutEscrow } = {
      ...revealing,
      escrow_ref: key(2),
    }

    expect(disputeAccount.safeParse(withoutEscrow).success).toBe(false)
  })

  it('fails on a deadline that does not fit a safe integer', () => {
    const parsed = disputeAccount.safeParse({
      ...revealing,
      commit_deadline: new BN('9007199254740993'),
    })

    expect(parsed.success).toBe(false)
  })

  it('fails on an object carrying two variants', () => {
    const parsed = disputeAccount.safeParse({
      ...revealing,
      state: { Committing: {}, Revealing: {} },
    })

    expect(parsed.success).toBe(false)
  })

  it('fails on a state the contract does not have', () => {
    expect(disputeAccount.safeParse({ ...revealing, state: { Withdrawn: {} } }).success).toBe(false)
  })

  it('drops the policy snapshot instead of choking on it', () => {
    const parsed = disputeAccount.safeParse(revealing)
    expect(parsed.success).toBe(true)
    expect(parsed.success && 'policy' in parsed.data).toBe(false)
  })
})

describe('disputes mentioned in logs', () => {
  const parser = new EventParser(programId, coder)

  const opened = encodeEvent('DisputeOpened', {
    dispute: key(9),
    integrator: key(1),
    escrow_ref: key(2),
    claimant: key(3),
    respondent: key(4),
    amount: new BN(5),
    optimistic: false,
    opened_at: new BN(1),
  })
  const committed = encodeEvent('VoteCommitted', { dispute: key(9), juror: key(7) })
  const otherCommitted = encodeEvent('VoteCommitted', { dispute: key(8), juror: key(7) })
  const staked = encodeEvent('JurorStaked', {
    juror: key(7),
    stake: new BN(1),
    index: 0,
    juror_count: 1,
  })

  it('collects addresses without repeats', () => {
    const found = disputesInLogs(parser, invocation(programId, opened, committed, otherCommitted))
    expect(found).toEqual([key(9).toBase58(), key(8).toBase58()])
  })

  /** The juror registry is not a dispute: these two have no `dispute` field. */
  it('does not mistake a registry event for a dispute', () => {
    expect(disputesInLogs(parser, invocation(programId, staked))).toEqual([])
  })

  /**
   * The same event emitted by another program in the same transaction must not
   * become a reason to read an account: a log subscription hands over the whole
   * transaction.
   */
  it('ignores events of another program', () => {
    const stranger = new PublicKey(new Uint8Array(32).fill(42))
    expect(disputesInLogs(parser, invocation(stranger, opened))).toEqual([])
  })

  it('survives a truncated log quietly', () => {
    const truncated = [`Program ${programId.toBase58()} invoke [1]`, 'Log truncated']
    expect(disputesInLogs(parser, truncated)).toEqual([])
  })

  it('survives a line that is not an event quietly', () => {
    const noise = invocation(programId, Buffer.from('not an event').toString('base64'))
    expect(disputesInLogs(parser, noise)).toEqual([])
  })
})

describe('the freshest snapshot per dispute', () => {
  const row = (pda: string, syncedSlot: number): DisputeRow => ({
    pda,
    integrator: key(1).toBase58(),
    escrowRef: key(2).toBase58(),
    claimant: key(3).toBase58(),
    respondent: key(4).toBase58(),
    amount: 1n,
    state: 'Committing',
    panel: [],
    reportHash: null,
    claimantClaimHash: '0'.repeat(64),
    respondentClaimHash: '0'.repeat(64),
    openedAt: 1,
    commitDeadline: 2,
    revealDeadline: 3,
    appealDeadline: 0,
    votesClaimant: 0,
    votesRespondent: 0,
    escalated: false,
    verdict: null,
    syncedSlot,
  })

  it('leaves one row per dispute', () => {
    const kept = latestPerDispute([row('a', 10), row('b', 10), row('a', 12)])
    expect(kept).toHaveLength(2)
    expect(kept.map((each) => [each.pda, each.syncedSlot])).toEqual([
      ['a', 12],
      ['b', 10],
    ])
  })

  it('does not let an older snapshot push out a newer one', () => {
    const kept = latestPerDispute([row('a', 99), row('a', 3)])
    expect(kept.map((each) => each.syncedSlot)).toEqual([99])
  })
})

describe('watcher', () => {
  const log: WatcherLog = { info: vi.fn(), warn: vi.fn(), error: vi.fn() }

  const chainOf = (overrides: Partial<Chain> = {}): Chain => ({
    allDisputes: vi.fn(async () => ({ slot: 100, accounts: [] })),
    readDispute: vi.fn(async () => null),
    subscribeLogs: vi.fn(async () => async () => {}),
    ...overrides,
  })

  beforeEach(() => {
    vi.clearAllMocks()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('rewrites the whole mirror before it starts listening', async () => {
    const account = await encodeDispute()
    const saved: DisputeRow[][] = []
    const chain = chainOf({
      allDisputes: vi.fn(async () => ({ slot: 500, accounts: [account] })),
    })

    const watcher = createWatcher({
      chain,
      cache: { save: async (rows) => void saved.push([...rows]) },
      log,
      programId,
      resyncIntervalMs: 0,
    })
    await watcher.start()

    expect(saved).toHaveLength(1)
    expect(saved[0]?.[0]?.syncedSlot).toBe(500)
    expect(chain.subscribeLogs).toHaveBeenCalledTimes(1)
  })

  /**
   * The mirror is a cache and the source of truth stays on chain. A service
   * that refused to come up over a one-minute RPC hiccup costs more than a
   * mirror one rewrite cycle out of date.
   */
  it('subscribes even when the first rewrite failed', async () => {
    const chain = chainOf({
      allDisputes: vi.fn(async () => {
        throw new Error('rpc down')
      }),
    })

    const watcher = createWatcher({
      chain,
      cache: { save: async () => {} },
      log,
      programId,
      resyncIntervalMs: 0,
    })
    await watcher.start()

    expect(chain.subscribeLogs).toHaveBeenCalledTimes(1)
    expect(log.error).toHaveBeenCalled()
  })

  it('reads the account on an event instead of retelling the event', async () => {
    const account = await encodeDispute()
    const saved: DisputeRow[] = []
    const delivered: ((logs: readonly string[]) => void)[] = []

    const chain = chainOf({
      readDispute: vi.fn(async (address: string) => ({
        slot: 900,
        account: { ...account, address },
      })),
      subscribeLogs: vi.fn(async (onLogs: (logs: readonly string[]) => void) => {
        delivered.push(onLogs)
        return async () => {}
      }),
    })

    const watcher = createWatcher({
      chain,
      cache: { save: async (rows) => void saved.push(...rows) },
      log,
      programId,
      resyncIntervalMs: 0,
    })
    await watcher.start()

    const committed = encodeEvent('VoteCommitted', { dispute: key(9), juror: key(7) })
    delivered[0]?.(invocation(programId, committed))
    await vi.waitFor(() => expect(saved).toHaveLength(1))

    expect(chain.readDispute).toHaveBeenCalledWith(key(9).toBase58())
    // The row has deadlines although the event carried none: they came from the
    // account.
    expect(saved[0]?.revealDeadline).toBe(1_700_000_120)
    expect(saved[0]?.syncedSlot).toBe(900)
  })

  it('reads no accounts when the logs hold no disputes', async () => {
    const delivered: ((logs: readonly string[]) => void)[] = []
    const chain = chainOf({
      subscribeLogs: vi.fn(async (onLogs: (logs: readonly string[]) => void) => {
        delivered.push(onLogs)
        return async () => {}
      }),
    })

    const watcher = createWatcher({
      chain,
      cache: { save: async () => {} },
      log,
      programId,
      resyncIntervalMs: 0,
    })
    await watcher.start()
    delivered[0]?.([`Program ${programId.toBase58()} invoke [1]`, 'Log truncated'])

    expect(chain.readDispute).not.toHaveBeenCalled()
  })

  it('skips a dispute RPC could not find and says so out loud', async () => {
    const chain = chainOf({ readDispute: vi.fn(async () => null) })
    const save = vi.fn(async () => {})

    const watcher = createWatcher({
      chain,
      cache: { save },
      log,
      programId,
      resyncIntervalMs: 0,
    })

    expect(await watcher.refresh([key(9).toBase58()])).toBe(0)
    expect(save).not.toHaveBeenCalled()
    expect(log.warn).toHaveBeenCalled()
  })

  it('does not take down the process when a read behind an event failed', async () => {
    const delivered: ((logs: readonly string[]) => void)[] = []
    const chain = chainOf({
      readDispute: vi.fn(async () => {
        throw new Error('rpc down')
      }),
      subscribeLogs: vi.fn(async (onLogs: (logs: readonly string[]) => void) => {
        delivered.push(onLogs)
        return async () => {}
      }),
    })

    const watcher = createWatcher({
      chain,
      cache: { save: async () => {} },
      log,
      programId,
      resyncIntervalMs: 0,
    })
    await watcher.start()

    const committed = encodeEvent('VoteCommitted', { dispute: key(9), juror: key(7) })
    expect(() => delivered[0]?.(invocation(programId, committed))).not.toThrow()
    await vi.waitFor(() => expect(log.error).toHaveBeenCalled())
  })

  /**
   * The scheduled rewrite is the only answer to a truncated log and to a
   * dropped WebSocket subscription: an event that was not in the logs is never
   * going to be delivered by anyone.
   */
  it('rewrites the mirror on a schedule', async () => {
    vi.useFakeTimers()
    const chain = chainOf()

    const watcher = createWatcher({
      chain,
      cache: { save: async () => {} },
      log,
      programId,
      resyncIntervalMs: 60_000,
    })
    await watcher.start()
    expect(chain.allDisputes).toHaveBeenCalledTimes(1)

    await vi.advanceTimersByTimeAsync(180_000)
    expect(chain.allDisputes).toHaveBeenCalledTimes(4)

    await watcher.stop()
    await vi.advanceTimersByTimeAsync(180_000)
    expect(chain.allDisputes).toHaveBeenCalledTimes(4)
  })

  /**
   * One transaction touches several disputes. A read that failed on one of them
   * must not take the successful ones with it: the next rewrite would bring
   * them back, of course, but until then the panel would be showing the old
   * state of two disputes instead of one.
   */
  it('keeps the successful snapshots when one read failed', async () => {
    const account = await encodeDispute()
    const saved: DisputeRow[] = []
    const chain = chainOf({
      readDispute: vi.fn(async (address: string) => {
        if (address === key(8).toBase58()) throw new Error('rpc down')
        return { slot: 900, account: { ...account, address } }
      }),
    })

    const watcher = createWatcher({
      chain,
      cache: { save: async (rows) => void saved.push(...rows) },
      log,
      programId,
      resyncIntervalMs: 0,
    })

    const stored = await watcher.refresh([key(8).toBase58(), key(9).toBase58()])

    expect(stored).toBe(1)
    expect(saved.map((row) => row.pda)).toEqual([key(9).toBase58()])
    expect(log.error).toHaveBeenCalled()
  })

  it('unsubscribes on stop', async () => {
    const unsubscribe = vi.fn(async () => {})
    const chain = chainOf({ subscribeLogs: vi.fn(async () => unsubscribe) })

    const watcher = createWatcher({
      chain,
      cache: { save: async () => {} },
      log,
      programId,
      resyncIntervalMs: 0,
    })
    await watcher.start()
    await watcher.stop()
    await watcher.stop()

    expect(unsubscribe).toHaveBeenCalledTimes(1)
  })
})
