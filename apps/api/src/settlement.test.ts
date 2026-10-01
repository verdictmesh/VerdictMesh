import { BN, BorshCoder } from '@coral-xyz/anchor'
import { PublicKey } from '@solana/web3.js'
import { describe, expect, it, vi } from 'vitest'
import type { ChainTransaction, SignatureInfo } from './evidence.js'
import { referenceEscrowIdl } from './idl/reference-escrow.js'
import {
  createSettlements,
  type DecidedDispute,
  type SettlementChain,
  type SettlementRow,
  type SettlementStore,
  type SettlingEscrow,
  settlementsIn,
} from './settlement.js'

/** Bytes from the coder of the build's IDL, snake_case — as in `evidence.test.ts`. */
const coder = new BorshCoder(referenceEscrowIdl)
const key = (fill: number) => new PublicKey(new Uint8Array(32).fill(fill))

const escrowProgram = key(200)
const foreignProgram = key(202)
const escrows: SettlingEscrow[] = [
  {
    name: 'reference_escrow',
    programId: escrowProgram,
    idl: referenceEscrowIdl,
    settledEvent: 'MilestoneSettled',
  },
]

const integrator = key(30).toBase58()
const foreignIntegrator = key(31).toBase58()
const dispute = key(13)

const settledLine = (forDispute: PublicKey) => {
  const event = referenceEscrowIdl.events?.find((e) => e.name === 'MilestoneSettled')
  if (!event) throw new Error('No MilestoneSettled in the IDL')
  const bytes = Buffer.concat([
    Buffer.from(event.discriminator),
    coder.types.encode('MilestoneSettled', {
      escrow: key(10),
      milestone: 1,
      dispute: forDispute,
      winner: key(11),
      amount: new BN(5_000_000),
      reimbursed: new BN(0),
    }),
  ])
  return `Program data: ${bytes.toString('base64')}`
}

const invocation = (program: PublicKey, ...lines: string[]) => [
  `Program ${program.toBase58()} invoke [1]`,
  ...lines,
  `Program ${program.toBase58()} success`,
]

const sig = (n: number) => String(n).repeat(88).slice(0, 88)

describe('settlementsIn', () => {
  it('reads the dispute out of the escrow settlement event', () => {
    expect(
      settlementsIn(invocation(escrowProgram, settledLine(dispute)), sig(1), 77, escrows),
    ).toEqual([
      {
        disputePda: dispute.toBase58(),
        escrowProgram: escrowProgram.toBase58(),
        signature: sig(1),
        slot: 77,
      },
    ])
  })

  it('does not let another program speak for the escrow', () => {
    expect(
      settlementsIn(invocation(foreignProgram, settledLine(dispute)), sig(1), 77, escrows),
    ).toEqual([])
  })

  it('ignores the escrow events that are not settlements', () => {
    const disputed = referenceEscrowIdl.events?.find((e) => e.name === 'MilestoneDisputed')
    if (!disputed) throw new Error('No MilestoneDisputed in the IDL')
    const line = `Program data: ${Buffer.concat([
      Buffer.from(disputed.discriminator),
      coder.types.encode('MilestoneDisputed', {
        escrow: key(10),
        milestone: 1,
        dispute,
        claimant: key(11),
        amount: new BN(5_000_000),
      }),
    ]).toString('base64')}`
    expect(settlementsIn(invocation(escrowProgram, line), sig(1), 77, escrows)).toEqual([])
  })
})

interface Fixture {
  signatures?: Record<string, SignatureInfo[]>
  transactions?: Record<string, ChainTransaction>
  stored?: string[]
}

const setup = (fixture: Fixture = {}) => {
  const saved: SettlementRow[] = []
  const subscriptions: ((signature: string, slot: number, logs: readonly string[]) => void)[] = []

  const chain: SettlementChain = {
    signaturesFor: vi.fn(async (address: string) => fixture.signatures?.[address] ?? []),
    readTransaction: vi.fn(async (signature: string) => fixture.transactions?.[signature] ?? null),
    subscribeProgramLogs: vi.fn(async (_program, onLogs) => {
      subscriptions.push(onLogs)
      return async () => {}
    }),
    integratorEscrows: vi.fn(
      async (addresses: readonly string[]) =>
        new Map(
          addresses.map((address) => [
            address,
            address === integrator ? escrowProgram.toBase58() : foreignProgram.toBase58(),
          ]),
        ),
    ),
  }
  const store: SettlementStore = {
    save: vi.fn(async (row: SettlementRow) => {
      saved.push(row)
    }),
    settled: vi.fn(
      async (pdas: readonly string[]) =>
        new Set(pdas.filter((pda) => fixture.stored?.includes(pda))),
    ),
  }
  let clock = 0
  const settlements = createSettlements({
    chain,
    store,
    escrows,
    log: { info: vi.fn(), warn: vi.fn() },
    refreshMs: 60_000,
    now: () => clock,
  })

  return {
    chain,
    store,
    saved,
    subscriptions,
    settlements,
    advance: (ms: number) => {
      clock += ms
    },
  }
}

const decided = (pda: PublicKey, owner = integrator): DecidedDispute => ({
  pda: pda.toBase58(),
  integrator: owner,
  verdict: 'Claimant',
})

const settleTx = (forDispute: PublicKey, slot: number): ChainTransaction => ({
  slot,
  blockTime: 1_700_000_000,
  signers: [key(40).toBase58()],
  logs: invocation(escrowProgram, settledLine(forDispute)),
})

describe('createSettlements', () => {
  it('finds a settlement made while the service slept, by the dispute address', async () => {
    const t = setup({
      signatures: {
        [dispute.toBase58()]: [
          { signature: sig(3), slot: 30, failed: false },
          { signature: sig(2), slot: 20, failed: false },
        ],
      },
      transactions: {
        [sig(3)]: { ...settleTx(key(99), 30) },
        [sig(2)]: settleTx(dispute, 20),
      },
    })

    t.settlements.consider([decided(dispute)])
    await t.settlements.idle()

    expect(t.saved).toEqual([
      {
        disputePda: dispute.toBase58(),
        escrowProgram: escrowProgram.toBase58(),
        signature: sig(2),
        slot: 20,
      },
    ])
  })

  it('skips failed transactions without reading them', async () => {
    const t = setup({
      signatures: { [dispute.toBase58()]: [{ signature: sig(3), slot: 30, failed: true }] },
      transactions: { [sig(3)]: settleTx(dispute, 30) },
    })

    t.settlements.consider([decided(dispute)])
    await t.settlements.idle()

    expect(t.chain.readTransaction).not.toHaveBeenCalled()
    expect(t.saved).toEqual([])
  })

  it('does not look for a dispute that has no verdict yet', async () => {
    const t = setup()
    t.settlements.consider([{ ...decided(dispute), verdict: null }])
    await t.settlements.idle()
    expect(t.chain.signaturesFor).not.toHaveBeenCalled()
  })

  it('does not look for a dispute whose escrow it cannot read', async () => {
    const t = setup()
    t.settlements.consider([decided(dispute, foreignIntegrator)])
    await t.settlements.idle()
    expect(t.chain.signaturesFor).not.toHaveBeenCalled()
  })

  it('does not look again for a settlement already stored', async () => {
    const t = setup({ stored: [dispute.toBase58()] })
    t.settlements.consider([decided(dispute)])
    await t.settlements.idle()
    expect(t.chain.signaturesFor).not.toHaveBeenCalled()
  })

  /** The sweep offers every decided dispute every five minutes; RPC pays once. */
  it('looks up an unsettled dispute once per process from snapshots', async () => {
    const t = setup()
    t.settlements.consider([decided(dispute)])
    await t.settlements.idle()
    t.advance(10 * 60_000)
    t.settlements.consider([decided(dispute)])
    await t.settlements.idle()
    expect(t.chain.signaturesFor).toHaveBeenCalledTimes(1)
  })

  it('looks again on demand, but not more than once per refresh interval', async () => {
    const t = setup()
    t.settlements.consider([decided(dispute)])
    await t.settlements.idle()

    t.settlements.refresh(decided(dispute))
    await t.settlements.idle()
    expect(t.chain.signaturesFor).toHaveBeenCalledTimes(1)

    t.advance(60_000)
    t.settlements.refresh(decided(dispute))
    await t.settlements.idle()
    expect(t.chain.signaturesFor).toHaveBeenCalledTimes(2)
  })

  it('stores a settlement seen live in the escrow logs', async () => {
    const t = setup()
    await t.settlements.start()
    t.subscriptions[0]?.(sig(5), 50, invocation(escrowProgram, settledLine(dispute)))
    await vi.waitFor(() => expect(t.saved).toHaveLength(1))
    expect(t.saved[0]?.signature).toBe(sig(5))
  })

  it('reads each integrator once and tells tracked from untracked', async () => {
    const t = setup()
    const first = await t.settlements.tracks([integrator, foreignIntegrator])
    await t.settlements.tracks([integrator])
    expect(first.get(integrator)).toBe(true)
    expect(first.get(foreignIntegrator)).toBe(false)
    expect(t.chain.integratorEscrows).toHaveBeenCalledTimes(1)
  })
})
