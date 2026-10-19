import { createHash } from 'node:crypto'
import { BN, BorshCoder, type Idl, utils } from '@coral-xyz/anchor'
import { PublicKey } from '@solana/web3.js'
import { verdictMeshIdl } from '@verdictmesh/sdk'
import { claimOf } from '../claims.js'
import {
  type ChainTransaction,
  collectEvidence,
  type EvidenceChain,
  type EvidenceSet,
  type KnownProgram,
  type SignatureInfo,
} from '../evidence.js'
import { referenceEscrowIdl } from '../idl/reference-escrow.js'

/**
 * A modelled dispute for `SC-007`: an escrow history written as steps, played
 * into the logs and account bytes a node would return, and collected by the
 * real `collectEvidence`. The model then sees exactly what it would see on
 * devnet — only the chain underneath is a script, so the answer is known in
 * advance and every trap is placed on purpose.
 *
 * Bytes come from the same `BorshCoder` that decodes them, fed the raw IDL
 * (snake_case fields): fed camelCase, the encoder writes zeroes instead of
 * failing (`evidence.test.ts`).
 */

export type Role = 'buyer' | 'seller'
export type Verdict = 'Claimant' | 'Respondent' | 'StatusQuo'

export type Step =
  /** The buyer accepts a milestone without a dispute. */
  | { release: number }
  /** An earlier dispute over a milestone, opened and then settled. */
  | { dispute: number; claimant: Role; verdict: Verdict }

export interface Scenario {
  /** Directory name under `fixtures/disputes/`. */
  id: string
  title: string
  /**
   * The false statements the case is built to tempt — what a careless report
   * would mark as confirmed. For the person labelling the report; the judge
   * does not read them.
   */
  traps: string[]
  milestones: bigint[]
  bond: bigint
  /** `Policy.deposit` the initiator pays on opening. */
  deposit: bigint
  dealId: bigint
  /** Before the dispute under report, oldest first. */
  history: Step[]
  dispute: { milestone: number; claimant: Role }
  /** Unix seconds of the escrow's creation. */
  createdAt: number
  /** Seconds between consecutive transactions. One number, or one per gap. */
  spacing?: number | number[]
  /** The escrow is owned by a program whose IDL the reporter does not have. */
  foreignEscrow?: boolean
  /** Transactions (by order, 0 = creation) whose logs the node cut. */
  logsCut?: number[]
  /** `collectEvidence`'s cap on transactions; the oldest are dropped. */
  maxTransactions?: number
  /** Who pays the fees; the parties sign only what needs them. */
  relayer?: 'separate' | 'claimant'
}

/** The program ids of the build, as deployed on devnet. */
export const ESCROW_PROGRAM = new PublicKey(referenceEscrowIdl.address)
export const MESH_PROGRAM = new PublicKey(verdictMeshIdl.address)
const FOREIGN_PROGRAM = keyOf('foreign-escrow-program')
const SYSTEM_PROGRAM = '11111111111111111111111111111111'
const TOKEN_PROGRAM = 'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA'

const escrowCoder = new BorshCoder(referenceEscrowIdl)
const meshCoder = new BorshCoder(verdictMeshIdl)

export function keyOf(seed: string): PublicKey {
  return new PublicKey(createHash('sha256').update(`sc007/${seed}`).digest())
}

function signatureOf(seed: string): string {
  return utils.bytes.bs58.encode(createHash('sha512').update(`sc007/sig/${seed}`).digest())
}

function encodeEvent(idl: Idl, coder: BorshCoder, name: string, data: Record<string, unknown>) {
  const event = idl.events?.find((candidate) => candidate.name === name)
  if (!event) throw new Error(`No such event in the IDL: ${name}`)
  const bytes = Buffer.concat([Buffer.from(event.discriminator), coder.types.encode(name, data)])
  return `Program data: ${bytes.toString('base64')}`
}

const escrowEvent = (name: string, data: Record<string, unknown>) =>
  encodeEvent(referenceEscrowIdl, escrowCoder, name, data)
const meshEvent = (name: string, data: Record<string, unknown>) =>
  encodeEvent(verdictMeshIdl, meshCoder, name, data)

function invoke(program: string, depth: number, instruction: string, body: string[]): string[] {
  return [
    `Program ${program} invoke [${depth}]`,
    `Program log: Instruction: ${instruction}`,
    ...body,
    `Program ${program} consumed ${20_000 + body.length * 3_100} of 200000 compute units`,
    `Program ${program} success`,
  ]
}

const transfer = (depth: number) => invoke(TOKEN_PROGRAM, depth, 'TransferChecked', [])
const createAccount = (depth: number) => [
  `Program ${SYSTEM_PROGRAM} invoke [${depth}]`,
  `Program ${SYSTEM_PROGRAM} success`,
]

/** What the escrow account holds for one milestone. */
type MilestoneState =
  | { Pending: Record<string, never> }
  | { Disputed: { dispute: PublicKey } }
  | { Released: Record<string, never> }
  | { Refunded: Record<string, never> }

/** One line of the answer key: what a transaction did, in plain words. */
export interface TruthTransaction {
  signature: string
  slot: number
  blockTime: number
  what: string
}

export interface Truth {
  title: string
  traps: string[]
  buyer: string
  seller: string
  claimant: Role
  respondent: Role
  dispute: string
  disputedMilestone: number
  disputedAmount: string
  bond: string
  deposit: string
  total: string
  /** The milestones as the account snapshot shows them, and every state each has been in. */
  milestones: { index: number; amount: string; state: string; passed: string[] }[]
  /** Every transaction that touched the escrow, oldest first — collected or not. */
  transactions: TruthTransaction[]
  /** What the reporter was given; differs from `transactions` when history was cut. */
  collected: { transactions: number; truncated: boolean; logsCut: number }
  escrowProgramKnown: boolean
}

/** The dispute as the reporter reads it from the mirror. */
export interface FixtureDispute {
  pda: string
  escrowRef: string
  claimant: string
  respondent: string
  amount: string
  openedAt: number
  claimantClaimHash: string
  respondentClaimHash: string
}

export interface Built {
  dispute: FixtureDispute
  evidence: EvidenceSet
  truth: Truth
}

/** The reporter's view of the programs: the escrow's IDL and its position formula. */
export function knownPrograms(positions: KnownProgram['positions']): KnownProgram[] {
  return [
    { name: 'verdict_mesh', programId: MESH_PROGRAM, idl: verdictMeshIdl },
    {
      name: 'reference_escrow',
      programId: ESCROW_PROGRAM,
      idl: referenceEscrowIdl,
      ...(positions ? { positions } : {}),
    },
  ]
}

const positionOf = (role: Role) => (role === 'seller' ? 'release' : 'refund')
const other = (role: Role): Role => (role === 'buyer' ? 'seller' : 'buyer')

export async function build(scenario: Scenario, programs: readonly KnownProgram[]): Promise<Built> {
  const seed = (name: string) => `${scenario.id}/${name}`
  const buyer = keyOf(seed('buyer'))
  const seller = keyOf(seed('seller'))
  const relayer = keyOf(seed('relayer'))
  const escrow = keyOf(seed('escrow'))
  const integrator = keyOf(seed('integrator'))
  const mint = keyOf(seed('mint'))
  const settlementMint = keyOf('settlement-mint')
  const jurors = Array.from({ length: 5 }, (_, index) => keyOf(`juror/${index}`))
  const escrowProgram = scenario.foreignEscrow ? FOREIGN_PROGRAM : ESCROW_PROGRAM
  const escrowId = escrowProgram.toBase58()
  const meshId = MESH_PROGRAM.toBase58()
  const party = (role: Role) => (role === 'buyer' ? buyer : seller)
  const feePayer = (role: Role) => (scenario.relayer === 'claimant' ? party(role) : relayer)

  const states: MilestoneState[] = scenario.milestones.map(() => ({ Pending: {} }))
  const stateName = (state: MilestoneState) => Object.keys(state)[0] ?? 'unknown'
  const passed: string[][] = scenario.milestones.map(() => ['Pending'])
  const enter = (milestone: number, state: MilestoneState) => {
    states[milestone] = state
    passed[milestone]?.push(stateName(state))
  }
  const transactions: (ChainTransaction & { signature: string; what: string })[] = []
  const total = scenario.milestones.reduce((sum, amount) => sum + amount, 0n)

  let slot = 505_000_000 + Number.parseInt(scenario.id, 10) * 10_000
  let time = scenario.createdAt
  const gap = (index: number) =>
    Array.isArray(scenario.spacing)
      ? (scenario.spacing[index] ?? scenario.spacing.at(-1) ?? 60)
      : (scenario.spacing ?? 60)

  const push = (name: string, signers: PublicKey[], logs: string[], what: string) => {
    if (transactions.length > 0) {
      const seconds = gap(transactions.length - 1)
      time += seconds
      slot += Math.max(1, Math.round(seconds / 0.4))
    }
    transactions.push({
      signature: signatureOf(seed(name)),
      slot,
      blockTime: time,
      signers: signers.map((key) => key.toBase58()),
      logs,
      what,
    })
  }

  push(
    'create',
    [relayer, buyer, seller],
    [
      ...invoke(escrowId, 1, 'CreateEscrow', [
        ...createAccount(2),
        ...transfer(2),
        ...transfer(2),
        ...transfer(2),
        escrowEvent('EscrowOpened', {
          escrow,
          buyer,
          seller,
          mint,
          integrator,
          total: new BN(total.toString()),
          milestones: scenario.milestones.length,
          bond: new BN(scenario.bond.toString()),
        }),
      ]),
    ],
    `Escrow created: buyer ${buyer.toBase58()}, seller ${seller.toBase58()}, total ${total}, ${scenario.milestones.length} milestone(s), bond ${scenario.bond} per side per milestone.`,
  )

  const open = (name: string, milestone: number, claimantRole: Role, dispute: PublicKey) => {
    const amount = scenario.milestones[milestone]
    if (amount === undefined) throw new Error(`${scenario.id}: no milestone ${milestone}`)
    const claimant = party(claimantRole)
    const respondent = party(other(claimantRole))
    const panel = jurors.slice(0, 3)
    enter(milestone, { Disputed: { dispute } })

    push(
      name,
      claimant.equals(feePayer(claimantRole)) ? [claimant] : [feePayer(claimantRole), claimant],
      [
        ...invoke(escrowId, 1, 'DisputeMilestone', [
          ...invoke(meshId, 2, 'OpenDispute', [
            ...createAccount(3),
            ...createAccount(3),
            ...transfer(3),
            meshEvent('DepositCollected', {
              dispute,
              depositor: claimant,
              amount: new BN(scenario.deposit.toString()),
            }),
            meshEvent('DisputeOpened', {
              dispute,
              integrator,
              escrow_ref: escrow,
              claimant,
              respondent,
              amount: new BN(amount.toString()),
              optimistic: false,
              opened_at: new BN(time + gap(transactions.length - 1)),
            }),
          ]),
          escrowEvent('MilestoneDisputed', {
            escrow,
            milestone,
            dispute,
            claimant,
            amount: new BN(amount.toString()),
          }),
        ]),
        ...invoke(meshId, 1, 'SelectPanel', [
          meshEvent('PanelSelected', {
            dispute,
            panel,
            entropy_slot: new BN(slot + 1),
          }),
        ]),
      ],
      `The ${claimantRole} opened dispute ${dispute.toBase58()} over milestone ${milestone} (${amount}), paid a deposit of ${scenario.deposit}; a panel of three was selected.`,
    )
    return { claimant, respondent, amount }
  }

  scenario.history.forEach((step, index) => {
    if ('release' in step) {
      const amount = scenario.milestones[step.release]
      if (amount === undefined) throw new Error(`${scenario.id}: no milestone ${step.release}`)
      enter(step.release, { Released: {} })
      push(
        `release-${index}`,
        [buyer],
        invoke(escrowId, 1, 'ReleaseMilestone', [
          ...transfer(2),
          ...transfer(2),
          ...transfer(2),
          escrowEvent('MilestoneReleased', {
            escrow,
            milestone: step.release,
            amount: new BN(amount.toString()),
          }),
        ]),
        `The buyer released milestone ${step.release} (${amount}) to the seller without a dispute.`,
      )
      return
    }

    const dispute = keyOf(seed(`earlier-dispute-${index}`))
    const { claimant, respondent, amount } = open(
      `earlier-open-${index}`,
      step.dispute,
      step.claimant,
      dispute,
    )
    const winner =
      step.verdict === 'Claimant' ? claimant : step.verdict === 'Respondent' ? respondent : null
    const reimbursed =
      step.verdict === 'Claimant'
        ? scenario.bond < scenario.deposit
          ? scenario.bond
          : scenario.deposit
        : 0n
    const paid = winner ? amount : 0n
    enter(
      step.dispute,
      winner ? (winner.equals(seller) ? { Released: {} } : { Refunded: {} }) : { Pending: {} },
    )
    const outcome = winner
      ? `${amount} paid to the ${winner.equals(seller) ? 'seller' : 'buyer'}, milestone ${winner.equals(seller) ? 'Released' : 'Refunded'}`
      : 'nothing paid, milestone back to Pending'

    push(
      `earlier-settle-${index}`,
      [relayer],
      invoke(escrowId, 1, 'SettleMilestone', [
        ...(winner ? transfer(2) : []),
        ...(winner ? [...transfer(2), ...transfer(2)] : []),
        escrowEvent('MilestoneSettled', {
          escrow,
          milestone: step.dispute,
          dispute,
          winner,
          amount: new BN(paid.toString()),
          reimbursed: new BN(reimbursed.toString()),
        }),
      ]),
      `Earlier dispute ${dispute.toBase58()} settled with verdict ${step.verdict}: ${outcome}; reimbursed ${reimbursed}.`,
    )
  })

  const disputePda = keyOf(seed('dispute'))
  const current = open('open', scenario.dispute.milestone, scenario.dispute.claimant, disputePda)
  const openedAt = transactions.at(-1)?.blockTime ?? time

  const cut = new Set(scenario.logsCut ?? [])
  const served: Record<string, ChainTransaction> = {}
  transactions.forEach(({ signature, what: _what, logs, ...transaction }, index) => {
    served[signature] = {
      ...transaction,
      logs: cut.has(index) && logs ? [...logs.slice(0, 3), 'Log truncated'] : logs,
    }
  })

  const accountData = await escrowCoder.accounts.encode('Escrow', {
    buyer,
    seller,
    mint,
    settlement_mint: settlementMint,
    integrator,
    bond: new BN(scenario.bond.toString()),
    deal_id: new BN(scenario.dealId.toString()),
    milestones: scenario.milestones.map((amount, index) => ({
      amount: new BN(amount.toString()),
      state: states[index],
    })),
    bump: 254,
  })
  const snapshotSlot = slot + 3

  const signatures: SignatureInfo[] = [...transactions]
    .reverse()
    .map(({ signature, slot: at }) => ({ signature, slot: at, failed: false }))

  const chain: EvidenceChain = {
    async readAccount(address) {
      if (address !== escrow.toBase58()) throw new Error(`unexpected account ${address}`)
      return { slot: snapshotSlot, owner: escrowId, data: accountData }
    },
    async signaturesFor(address, limit) {
      if (address !== escrow.toBase58()) throw new Error(`unexpected history of ${address}`)
      return signatures.slice(0, limit)
    },
    async readTransaction(signature) {
      const transaction = served[signature]
      if (!transaction) throw new Error(`unexpected read of ${signature}`)
      return transaction
    },
  }

  const evidence = await collectEvidence({
    chain,
    programs,
    target: { pda: disputePda.toBase58(), escrowRef: escrow.toBase58() },
    ...(scenario.maxTransactions ? { maxTransactions: scenario.maxTransactions } : {}),
    concurrency: 1,
  })

  // A foreign escrow words its claims in a way we cannot recover: the
  // fingerprints are still there, but no formula we have reproduces them.
  const claimHash = (role: Role) =>
    scenario.foreignEscrow
      ? createHash('sha256')
          .update(seed(`foreign-claim-${role}`))
          .digest('hex')
      : claimOf(escrow, scenario.dispute.milestone, positionOf(role))

  return {
    dispute: {
      pda: disputePda.toBase58(),
      escrowRef: escrow.toBase58(),
      claimant: current.claimant.toBase58(),
      respondent: current.respondent.toBase58(),
      amount: current.amount.toString(),
      openedAt,
      claimantClaimHash: claimHash(scenario.dispute.claimant),
      respondentClaimHash: claimHash(other(scenario.dispute.claimant)),
    },
    evidence,
    truth: {
      title: scenario.title,
      traps: scenario.traps,
      buyer: buyer.toBase58(),
      seller: seller.toBase58(),
      claimant: scenario.dispute.claimant,
      respondent: other(scenario.dispute.claimant),
      dispute: disputePda.toBase58(),
      disputedMilestone: scenario.dispute.milestone,
      disputedAmount: current.amount.toString(),
      bond: scenario.bond.toString(),
      deposit: scenario.deposit.toString(),
      total: total.toString(),
      milestones: scenario.milestones.map((amount, index) => ({
        index,
        amount: amount.toString(),
        state: stateName(states[index] ?? { Pending: {} }),
        passed: [...new Set(passed[index])],
      })),
      transactions: transactions.map(({ signature, slot: at, blockTime, what }) => ({
        signature,
        slot: at,
        blockTime: blockTime ?? 0,
        what,
      })),
      collected: {
        transactions: evidence.rows.filter((row) => row.kind === 'transaction').length,
        truncated: evidence.truncated,
        logsCut: cut.size,
      },
      escrowProgramKnown: !scenario.foreignEscrow,
    },
  }
}
