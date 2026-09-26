import { BN, BorshCoder, EventParser, type Idl } from '@coral-xyz/anchor'
import { PublicKey } from '@solana/web3.js'
import { describe, expect, it } from 'vitest'
import { referenceEscrowIdl } from './idl/reference-escrow.js'
import { verdictMeshIdl } from './idl/verdict-mesh.js'
import { parseLogs } from './logs.js'

/** Raw IDLs, snake_case fields — the form `BorshCoder` has to be fed. */
const meshCoder = new BorshCoder(verdictMeshIdl)
const escrowCoder = new BorshCoder(referenceEscrowIdl)

const key = (fill: number) => new PublicKey(new Uint8Array(32).fill(fill))

const mesh = key(201).toBase58()
const escrowProgram = key(200).toBase58()
const foreign = key(202).toBase58()
const token = 'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA'

const coders = new Map([
  [mesh, meshCoder],
  [escrowProgram, escrowCoder],
])

const dispute = key(13)
const escrow = key(10)

const data = (idl: Idl, coder: BorshCoder, name: string, fields: Record<string, unknown>) => {
  const event = idl.events?.find((candidate) => candidate.name === name)
  if (!event) throw new Error(`No such event in the IDL: ${name}`)
  const bytes = Buffer.concat([Buffer.from(event.discriminator), coder.types.encode(name, fields)])
  return `Program data: ${bytes.toString('base64')}`
}

const depositCollected = data(verdictMeshIdl, meshCoder, 'DepositCollected', {
  dispute,
  depositor: key(11),
  amount: new BN(5_000_000),
})

const disputeOpened = data(verdictMeshIdl, meshCoder, 'DisputeOpened', {
  dispute,
  integrator: key(15),
  escrow_ref: escrow,
  claimant: key(11),
  respondent: key(12),
  amount: new BN(30_000_000),
  optimistic: false,
  opened_at: new BN(1_700_000_020),
})

const milestoneDisputed = data(referenceEscrowIdl, escrowCoder, 'MilestoneDisputed', {
  escrow,
  milestone: 2,
  dispute,
  claimant: key(11),
  amount: new BN(30_000_000),
})

/**
 * The shape of a real `dispute_milestone` transaction on devnet: the escrow at
 * depth 1 calls `open_dispute` through CPI, which moves the deposit through the
 * token program at depth 3 and emits two events at depth 2; the escrow emits
 * its own event after the call returns.
 */
const opening = [
  `Program ${escrowProgram} invoke [1]`,
  'Program log: Instruction: DisputeMilestone',
  `Program ${mesh} invoke [2]`,
  'Program log: Instruction: OpenDispute',
  `Program ${token} invoke [3]`,
  `Program ${token} consumed 233 of 371435 compute units`,
  `Program ${token} success`,
  depositCollected,
  disputeOpened,
  `Program ${mesh} consumed 28636 of 389106 compute units`,
  `Program ${mesh} success`,
  milestoneDisputed,
  `Program ${escrowProgram} consumed 41591 of 400000 compute units`,
  `Program ${escrowProgram} success`,
]

describe('parseLogs', () => {
  it('decodes events of a program called through CPI', () => {
    const { events } = parseLogs(opening, coders)

    expect(events.map((event) => [event.programId, event.name])).toEqual([
      [mesh, 'DepositCollected'],
      [mesh, 'DisputeOpened'],
      [escrowProgram, 'MilestoneDisputed'],
    ])
    expect(events[1]?.data.escrow_ref).toEqual(escrow)
  })

  /**
   * A tripwire rather than a test of our code: the reason `parseLogs` exists.
   * When an Anchor upgrade makes this fail, `EventParser` has learnt to follow
   * CPI, and the parser here can be reconsidered.
   */
  it('exists because EventParser from anchor loses events emitted under CPI', () => {
    const parser = new EventParser(new PublicKey(mesh), meshCoder)
    expect([...parser.parseLogs([...opening])]).toEqual([])
  })

  it('does not let a foreign program speak with our discriminator', () => {
    const { events } = parseLogs(
      [`Program ${foreign} invoke [1]`, disputeOpened, `Program ${foreign} success`],
      coders,
    )

    expect(events).toEqual([])
  })

  it('returns to the caller after a nested program exits', () => {
    const { events } = parseLogs(
      [
        `Program ${mesh} invoke [1]`,
        `Program ${foreign} invoke [2]`,
        disputeOpened,
        `Program ${foreign} success`,
        depositCollected,
        `Program ${mesh} success`,
      ],
      coders,
    )

    expect(events.map((event) => event.name)).toEqual(['DepositCollected'])
  })

  it('treats a failed frame as an exit too', () => {
    const { events } = parseLogs(
      [
        `Program ${mesh} invoke [1]`,
        `Program ${foreign} invoke [2]`,
        `Program ${foreign} failed: custom program error: 0x1`,
        depositCollected,
        `Program ${mesh} success`,
      ],
      coders,
    )

    expect(events.map((event) => event.name)).toEqual(['DepositCollected'])
  })

  it('skips data that is not an event of the program', () => {
    const { events } = parseLogs(
      [`Program ${mesh} invoke [1]`, 'Program data: AAAA', `Program ${mesh} success`],
      coders,
    )

    expect(events).toEqual([])
  })

  it('lists every program that ran, at any depth, once', () => {
    const { programs } = parseLogs([...opening, ...opening], coders)
    expect(programs).toEqual([escrowProgram, mesh, token])
  })

  it('says so when the log hit its cap', () => {
    expect(parseLogs(opening, coders).truncated).toBe(false)
    expect(parseLogs([...opening.slice(0, 4), 'Log truncated'], coders).truncated).toBe(true)
  })
})
