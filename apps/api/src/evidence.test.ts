import { BN, BorshCoder, type Idl } from '@coral-xyz/anchor'
import { PublicKey } from '@solana/web3.js'
import { describe, expect, it, vi } from 'vitest'
import {
  type ChainTransaction,
  collectEvidence,
  type EvidenceChain,
  type KnownProgram,
  type SignatureInfo,
  toJson,
} from './evidence.js'
import { referenceEscrowIdl } from './idl/reference-escrow.js'
import { verdictMeshIdl } from './idl/verdict-mesh.js'

/**
 * As in the watcher tests, the bytes are produced by the same `BorshCoder`
 * that decodes them, from the raw IDL of the build. Field names are therefore
 * snake_case: fed camelCase, the encoder does not fail — it writes zeroes into
 * every field it cannot find, and a round trip would still look plausible.
 */
const escrowCoder = new BorshCoder(referenceEscrowIdl)
const meshCoder = new BorshCoder(verdictMeshIdl)

const key = (fill: number) => new PublicKey(new Uint8Array(32).fill(fill))

/** Deliberately not the addresses from the IDLs: `anchor` re-keys programs. */
const escrowProgram = key(200)
const meshProgram = key(201)
const foreignProgram = key(202)

const programs: KnownProgram[] = [
  { name: 'verdict_mesh', programId: meshProgram, idl: verdictMeshIdl },
  { name: 'reference_escrow', programId: escrowProgram, idl: referenceEscrowIdl },
]

const escrow = key(10)
const buyer = key(11)
const seller = key(12)
const dispute = key(13)
const target = { pda: dispute.toBase58(), escrowRef: escrow.toBase58() }

const encodeEvent = (idl: Idl, coder: BorshCoder, name: string, data: Record<string, unknown>) => {
  const event = idl.events?.find((candidate) => candidate.name === name)
  if (!event) throw new Error(`No such event in the IDL: ${name}`)
  const bytes = Buffer.concat([Buffer.from(event.discriminator), coder.types.encode(name, data)])
  return `Program data: ${bytes.toString('base64')}`
}

const invocation = (program: PublicKey, ...lines: string[]) => [
  `Program ${program.toBase58()} invoke [1]`,
  ...lines,
  `Program ${program.toBase58()} success`,
]

const escrowOpened = encodeEvent(referenceEscrowIdl, escrowCoder, 'EscrowOpened', {
  escrow,
  buyer,
  seller,
  mint: key(14),
  integrator: key(15),
  total: new BN('18446744073709551615'),
  milestones: 2,
  bond: new BN(50),
})

const milestoneDisputed = encodeEvent(referenceEscrowIdl, escrowCoder, 'MilestoneDisputed', {
  escrow,
  milestone: 1,
  dispute,
  claimant: buyer,
  amount: new BN(700),
})

const disputeOpened = encodeEvent(verdictMeshIdl, meshCoder, 'DisputeOpened', {
  dispute,
  integrator: key(15),
  escrow_ref: escrow,
  claimant: buyer,
  respondent: seller,
  amount: new BN(700),
  optimistic: false,
  opened_at: new BN(1_700_000_020),
})

const escrowAccount = async () =>
  escrowCoder.accounts.encode('Escrow', {
    buyer,
    seller,
    mint: key(14),
    settlement_mint: key(16),
    integrator: key(15),
    bond: new BN(50),
    deal_id: new BN(7),
    milestones: [
      { amount: new BN(300), state: { Released: {} } },
      { amount: new BN(700), state: { Disputed: { dispute } } },
    ],
    bump: 253,
  })

interface FakeChain extends EvidenceChain {
  transactionReads: string[]
  signatureLimits: number[]
}

/** Newest first, as `getSignaturesForAddress` returns them. */
const fakeChain = (options: {
  account?: { owner: PublicKey; data: Uint8Array } | null
  signatures: SignatureInfo[]
  transactions: Record<string, ChainTransaction | null>
}): FakeChain => {
  const transactionReads: string[] = []
  const signatureLimits: number[] = []

  return {
    transactionReads,
    signatureLimits,
    async readAccount(address) {
      expect(address).toBe(target.escrowRef)
      if (!options.account) return null
      return { slot: 900, owner: options.account.owner.toBase58(), data: options.account.data }
    },
    async signaturesFor(address, limit) {
      expect(address).toBe(target.escrowRef)
      signatureLimits.push(limit)
      return options.signatures.slice(0, limit)
    },
    async readTransaction(signature) {
      transactionReads.push(signature)
      const transaction = options.transactions[signature]
      if (transaction === undefined) throw new Error(`unexpected read of ${signature}`)
      return transaction
    },
  }
}

const signature = (name: string) => name.padEnd(64, 'x')

const tx = (
  slot: number,
  logs: string[] | null,
  signers = [buyer.toBase58()],
): ChainTransaction => ({
  slot,
  blockTime: 1_700_000_000 + slot,
  signers,
  logs,
})

describe('transactions as evidence', () => {
  /**
   * The shape of a real opening on devnet: the escrow calls VerdictMesh through
   * CPI, and `DisputeOpened` is emitted at depth 2. `EventParser` from anchor
   * loses exactly that event (see `logs.ts`).
   */
  it('stores one row per signature, even when it carries events of both programs', async () => {
    const opening = signature('opening')
    const chain = fakeChain({
      account: null,
      signatures: [{ signature: opening, slot: 20, failed: false }],
      transactions: {
        [opening]: tx(20, [
          `Program ${escrowProgram.toBase58()} invoke [1]`,
          ...invocation(meshProgram, disputeOpened).map((line) => line.replace('[1]', '[2]')),
          milestoneDisputed,
          `Program ${escrowProgram.toBase58()} success`,
        ]),
      },
    })

    const { rows } = await collectEvidence({ chain, programs, target })

    expect(rows).toHaveLength(1)
    expect(rows[0]).toMatchObject({
      disputePda: target.pda,
      kind: 'transaction',
      source: opening,
      slot: 20,
    })
    expect(rows[0]?.payload.events).toEqual([
      expect.objectContaining({ program: 'verdict_mesh', name: 'DisputeOpened' }),
      expect.objectContaining({ program: 'reference_escrow', name: 'MilestoneDisputed' }),
    ])
  })

  it('decodes event fields into plain JSON: base58 keys and u64 as decimal strings', async () => {
    const opened = signature('opened')
    const chain = fakeChain({
      account: null,
      signatures: [{ signature: opened, slot: 5, failed: false }],
      transactions: { [opened]: tx(5, invocation(escrowProgram, escrowOpened)) },
    })

    const { rows } = await collectEvidence({ chain, programs, target })

    expect(rows[0]?.payload).toEqual({
      blockTime: 1_700_000_005,
      signers: [buyer.toBase58()],
      programs: [escrowProgram.toBase58()],
      events: [
        {
          program: 'reference_escrow',
          name: 'EscrowOpened',
          data: {
            escrow: escrow.toBase58(),
            buyer: buyer.toBase58(),
            seller: seller.toBase58(),
            mint: key(14).toBase58(),
            integrator: key(15).toBase58(),
            total: '18446744073709551615',
            milestones: 2,
            bond: '50',
          },
        },
      ],
      logsTruncated: false,
    })
  })

  /**
   * The log parser follows the call stack, so an event of a program we do not
   * know stays undecoded — but the fact that the program ran is kept: for an
   * escrow we have no IDL for, that list is all the model gets.
   */
  it('keeps the invoked programs even when none of them is known', async () => {
    const foreign = signature('foreign')
    const chain = fakeChain({
      account: null,
      signatures: [{ signature: foreign, slot: 7, failed: false }],
      transactions: {
        [foreign]: tx(7, [
          ...invocation(foreignProgram, escrowOpened),
          `Program ${foreignProgram.toBase58()} invoke [1]`,
          `Program ${escrowProgram.toBase58()} invoke [2]`,
          `Program ${escrowProgram.toBase58()} success`,
          `Program ${foreignProgram.toBase58()} success`,
        ]),
      },
    })

    const { rows } = await collectEvidence({ chain, programs, target })

    expect(rows[0]?.payload.events).toEqual([])
    expect(rows[0]?.payload.programs).toEqual([foreignProgram.toBase58(), escrowProgram.toBase58()])
  })

  it('skips failed transactions without reading them', async () => {
    const failed = signature('failed')
    const ok = signature('ok')
    const chain = fakeChain({
      account: null,
      signatures: [
        { signature: failed, slot: 9, failed: true },
        { signature: ok, slot: 8, failed: false },
      ],
      transactions: { [ok]: tx(8, invocation(escrowProgram, escrowOpened)) },
    })

    const { rows } = await collectEvidence({ chain, programs, target })

    expect(rows.map((row) => row.source)).toEqual([ok])
    expect(chain.transactionReads).toEqual([ok])
  })

  it('marks a transaction whose log hit the 10 KB cap', async () => {
    const long = signature('long')
    const chain = fakeChain({
      account: null,
      signatures: [{ signature: long, slot: 3, failed: false }],
      transactions: {
        [long]: tx(3, [`Program ${escrowProgram.toBase58()} invoke [1]`, 'Log truncated']),
      },
    })

    const { rows } = await collectEvidence({ chain, programs, target })

    expect(rows[0]?.payload.logsTruncated).toBe(true)
  })

  /**
   * A partial set is indistinguishable from a complete one once it is in the
   * table: the model would call a fact unconfirmed only because its
   * transaction failed to load. So the collection fails as a whole.
   */
  it('fails as a whole when a confirmed transaction cannot be read', async () => {
    const ok = signature('ok')
    const missing = signature('missing')
    const chain = fakeChain({
      account: null,
      signatures: [
        { signature: ok, slot: 2, failed: false },
        { signature: missing, slot: 1, failed: false },
      ],
      transactions: { [ok]: tx(2, []), [missing]: null },
    })

    await expect(collectEvidence({ chain, programs, target })).rejects.toThrow(missing)
  })

  it('fails as a whole when a transaction arrives without logs', async () => {
    const bare = signature('bare')
    const chain = fakeChain({
      account: null,
      signatures: [{ signature: bare, slot: 2, failed: false }],
      transactions: { [bare]: tx(2, null) },
    })

    await expect(collectEvidence({ chain, programs, target })).rejects.toThrow(bare)
  })

  it('keeps the newest transactions over the cap and says it cut the rest', async () => {
    const signatures = Array.from({ length: 5 }, (_, index) => ({
      signature: signature(`s${index}`),
      slot: 100 - index,
      failed: false,
    }))
    const transactions = Object.fromEntries(
      signatures.map((entry) => [entry.signature, tx(entry.slot, [])]),
    )
    const chain = fakeChain({ account: null, signatures, transactions })

    const { rows, truncated } = await collectEvidence({
      chain,
      programs,
      target,
      maxTransactions: 3,
    })

    expect(truncated).toBe(true)
    expect(rows.map((row) => row.slot)).toEqual([100, 99, 98])
    expect(chain.transactionReads).toHaveLength(3)
    expect(chain.signatureLimits).toEqual([1_000])
  })

  it('says nothing was cut when the whole history fits', async () => {
    const only = signature('only')
    const chain = fakeChain({
      account: null,
      signatures: [{ signature: only, slot: 1, failed: false }],
      transactions: { [only]: tx(1, []) },
    })

    const { truncated } = await collectEvidence({ chain, programs, target })

    expect(truncated).toBe(false)
  })
})

describe('the escrow account as evidence', () => {
  it('decodes the account of a known program with its slot of the read', async () => {
    const chain = fakeChain({
      account: { owner: escrowProgram, data: await escrowAccount() },
      signatures: [],
      transactions: {},
    })

    const { rows } = await collectEvidence({ chain, programs, target })

    expect(rows).toEqual([
      expect.objectContaining({
        disputePda: target.pda,
        kind: 'account',
        source: target.escrowRef,
        slot: 900,
      }),
    ])
    expect(rows[0]?.payload).toMatchObject({
      owner: escrowProgram.toBase58(),
      program: 'reference_escrow',
      account: 'Escrow',
      data: {
        buyer: buyer.toBase58(),
        seller: seller.toBase58(),
        settlement_mint: key(16).toBase58(),
        bond: '50',
        deal_id: '7',
        milestones: [
          { amount: '300', state: { Released: {} } },
          { amount: '700', state: { Disputed: { dispute: dispute.toBase58() } } },
        ],
      },
    })
  })

  it('keeps the owner and the size of an account whose program it does not know', async () => {
    const chain = fakeChain({
      account: { owner: foreignProgram, data: new Uint8Array(120) },
      signatures: [],
      transactions: {},
    })

    const { rows } = await collectEvidence({ chain, programs, target })

    expect(rows[0]?.payload).toEqual({
      owner: foreignProgram.toBase58(),
      program: null,
      account: null,
      data: null,
      bytes: 120,
    })
  })

  /**
   * A known program with an account the vendored IDL cannot name means the IDL
   * and the deployed program have drifted apart. Stored undecoded, that would
   * read as "an escrow of an unknown kind" — a quieter lie than an error.
   */
  it('refuses an account of a known program that the IDL cannot name', async () => {
    const chain = fakeChain({
      account: { owner: escrowProgram, data: new Uint8Array(120).fill(7) },
      signatures: [],
      transactions: {},
    })

    await expect(collectEvidence({ chain, programs, target })).rejects.toThrow(/IDL/)
  })

  it('still collects the history of an account that no longer exists', async () => {
    const opened = signature('opened')
    const chain = fakeChain({
      account: null,
      signatures: [{ signature: opened, slot: 5, failed: false }],
      transactions: { [opened]: tx(5, invocation(escrowProgram, escrowOpened)) },
    })

    const { rows } = await collectEvidence({ chain, programs, target })

    expect(rows.map((row) => row.kind)).toEqual(['transaction'])
  })
})

describe('reading the chain', () => {
  it('never runs more reads at once than asked', async () => {
    let running = 0
    let peak = 0
    const signatures = Array.from({ length: 8 }, (_, index) => ({
      signature: signature(`p${index}`),
      slot: index,
      failed: false,
    }))
    const chain: EvidenceChain = {
      readAccount: async () => null,
      signaturesFor: async () => signatures,
      readTransaction: vi.fn(async (read: string) => {
        running += 1
        peak = Math.max(peak, running)
        await new Promise((resolve) => setTimeout(resolve, 1))
        running -= 1
        const found = signatures.find((entry) => entry.signature === read)
        return tx(found?.slot ?? 0, [])
      }),
    }

    const { rows } = await collectEvidence({ chain, programs, target, concurrency: 3 })

    expect(rows).toHaveLength(8)
    expect(peak).toBe(3)
  })
})

describe('toJson', () => {
  it('turns chain values into plain JSON', () => {
    expect(
      toJson({ key: key(1), amount: new BN('18446744073709551615'), none: null, list: [1, true] }),
    ).toEqual({
      key: key(1).toBase58(),
      amount: '18446744073709551615',
      none: null,
      list: [1, true],
    })
  })

  it('writes bytes as lowercase hex', () => {
    expect(toJson(Buffer.from([0, 171, 255]))).toBe('00abff')
  })

  it('refuses what JSON cannot carry faithfully', () => {
    expect(() => toJson(Number.NaN)).toThrow()
    expect(() => toJson(() => 1)).toThrow()
    expect(() => toJson(new Date(0))).toThrow()
  })
})
