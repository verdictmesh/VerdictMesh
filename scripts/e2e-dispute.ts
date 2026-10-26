/**
 * End-to-end run on devnet — `SC-001`, `SC-002`, `SC-005`.
 *
 * The script walks the full "dispute → vote → payout" cycle as many times as
 * `--disputes` asks and measures three things: how long a cycle takes, what its
 * fees cost all participants combined, and how many times a human had to step
 * in. The pass/fail verdict is computed by `./e2e.ts`, which is under tests,
 * because numbers from devnet arrive slowly and only once.
 *
 * The VerdictMesh side — addresses, the commitment, registration, and the
 * accounts and panel selection for opening a dispute — comes from
 * `@verdictmesh/sdk`; the escrow's own instructions are still built here. One
 * transaction carries two signatures: the escrow PDA opens the dispute via CPI,
 * while the claimant pays the hearing deposit with their own key (`FR-026`),
 * and without that signature VerdictMesh will not accept the dispute.
 *
 * `--escrow` picks the integration: `reference` (milestones, winner takes the
 * milestone, bond vault) or `depin` (a reward epoch split between network and
 * operator, final-offer dispute). Only three steps differ — locking the funds,
 * opening the dispute, executing the verdict — and they sit behind
 * `EscrowDriver`; the panel, the windows and the measurement are the same.
 *
 * **Every key in the run is derived from the treasury key.** Panel selection
 * takes the whole registry (`FR-006`), so random jurors generated in memory
 * would stay in it forever after every crash — staked, eligible for selection,
 * and with no private key. Deterministic keys make a rerun the same run: the
 * registry does not grow, and already-staked jurors are recognised.
 */

import { createHash, createPrivateKey, sign } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { parseArgs } from 'node:util'
import { AnchorProvider, BN, Program, Wallet } from '@coral-xyz/anchor'
import { keccak_256 } from '@noble/hashes/sha3'
import {
  createAssociatedTokenAccountIdempotentInstruction,
  createMintToInstruction,
  getAssociatedTokenAddressSync,
  getMint,
  TOKEN_PROGRAM_ID,
} from '@solana/spl-token'
import {
  Connection,
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  SystemProgram,
  sendAndConfirmTransaction,
  Transaction,
} from '@solana/web3.js'
import {
  ballotArg,
  commitmentOf,
  configAddress,
  disputeVaultAddress,
  integratorAddress,
  jurorAddress,
  jurorIndexAddress,
  type PreparedDispute,
  registryAddress,
  type Ballot as Side,
  stakeVaultAddress,
  VerdictMeshClient,
  voteAddress,
} from '@verdictmesh/sdk'
import bs58 from 'bs58'
import { z } from 'zod'
import { type Ballot, cycleFloorSeconds, expectedOutcome, measure, type Run } from './e2e.js'
import type { DepinEscrow } from './idl/depin_escrow.js'
import type { ReferenceEscrow } from './idl/reference_escrow.js'

const envSchema = z.object({
  SOLANA_RPC_URL: z.url(),
  SETTLEMENT_MINT: z.string().min(32).max(44),
  TREASURY_KEYPAIR: z.string().min(1),
})

const { values } = parseArgs({
  options: {
    disputes: { type: 'string', default: '20' },
    // The public devnet RPC already answers 429 at three parallel cycles: each
    // is a dozen transactions and as many confirmations. A wider wave only
    // makes sense against your own node.
    batch: { type: 'string', default: '3' },
    // One panel seat whose vote comes from outside the script — the web panel
    // driven by a person or a browser. The script waits for that vote instead
    // of sending it, and opens it itself only if nobody does, so a missed
    // reveal never slashes a juror the next run needs.
    'web-seat': { type: 'string' },
    'sol-price': { type: 'string', default: '200' },
    escrow: { type: 'string', default: 'reference' },
  },
})

if (values.escrow !== 'reference' && values.escrow !== 'depin') {
  throw new Error(`--escrow is reference or depin, not ${values.escrow}`)
}
const escrowKind: 'reference' | 'depin' = values.escrow

const env = envSchema.parse(process.env)
const disputeCount = Number(values.disputes)
const batchSize = Number(values.batch)
const solPriceUsd = Number(values['sol-price'])
const webSeat = values['web-seat'] === undefined ? null : Number(values['web-seat'])
if (webSeat !== null && disputeCount !== 1) {
  throw new Error('--web-seat hands one vote to a person; run it with --disputes 1')
}

const connection = new Connection(env.SOLANA_RPC_URL, 'confirmed')
const treasury = Keypair.fromSecretKey(bs58.decode(env.TREASURY_KEYPAIR))
const settlementMint = new PublicKey(env.SETTLEMENT_MINT)

const provider = new AnchorProvider(connection, new Wallet(treasury), {
  commitment: 'confirmed',
})

/**
 * The IDL comes from the fresh build, the type from the copy in `./idl` (see
 * `sync-idl.ts`). Drift between them becomes a red typecheck, not a surprise on
 * devnet.
 */
const readIdl = <T>(name: string): T =>
  JSON.parse(readFileSync(new URL(`../target/idl/${name}.json`, import.meta.url), 'utf8')) as T

const client = new VerdictMeshClient(provider)
const mesh = client.program
const referenceProgram = new Program<ReferenceEscrow>(
  readIdl<ReferenceEscrow>('reference_escrow'),
  provider,
)
const depinProgram = new Program<DepinEscrow>(readIdl<DepinEscrow>('depin_escrow'), provider)
const escrowProgramId =
  escrowKind === 'reference' ? referenceProgram.programId : depinProgram.programId

// ── demo configuration from docs/PLAN.md ────────────────────────────────────

const DECIMALS = 6
const unit = (amount: number) => new BN(amount * 10 ** DECIMALS)

const POLICY = {
  panelSize: 3,
  extendedPanelSize: 5,
  quorum: 2,
  extendedQuorum: 3,
  // An eligibility threshold, not "how much to deposit". 120 is above the 100
  // staked by the jurors a crashed run left in the devnet registry (their keys
  // are gone), and below what this run's jurors hold now. Eligibility is
  // measured against the dispute's policy (`FR-011a`), so the threshold is the
  // intended way to keep strangers off the panel. None of the three criteria
  // depends on the stake size.
  jurorStake: unit(120),
  slashBpsWrong: 1_000,
  slashBpsNoReveal: 2_000,
  commitWindow: new BN(60),
  revealWindow: new BN(60),
  appealWindow: new BN(90),
  optimisticWindow: new BN(60),
  deposit: unit(5),
  optimisticThreshold: unit(50),
}

const MILESTONES = [unit(10), unit(20), unit(30)]
const DISPUTED_MILESTONE = 1

/**
 * The panel votes unanimously, and it has to.
 *
 * A split panel is impossible over a twenty-dispute measurement: `stake`
 * creates the juror record via `init`, there is no way to top up a stake, and a
 * losing vote costs 10% (`FR-011`). A juror who lost a share drops below the
 * eligibility threshold and never lands on a panel again, so has no way to win
 * the loss back. A registry of three for a panel of three cannot assemble a
 * panel at all after the first split vote: `RegistryTooSmall`.
 *
 * This is a property of the design, not a run failure, and it is recorded as
 * finding T025. Slashing does not go unverified: it is proven by 26 tests in
 * `settle_stakes.rs` and by a separate single-dispute run where the panel
 * splits 2:1.
 */
const BALLOTS: Ballot[] = ['claimant', 'claimant', 'claimant']

/** The run's sides under the names the SDK and the IDL give them. */
const SIDE: Record<Ballot, Side> = { claimant: 'Claimant', respondent: 'Respondent' }

/**
 * The run's keys: the same treasury yields the same three jurors and the same
 * integrator authority. The treasury secret is part of the seed, so the keys
 * can only be derived on a machine that already controls it.
 */
function derivedKey(role: string, index: number): Keypair {
  return Keypair.fromSeed(
    keccak_256(
      Buffer.concat([
        Buffer.from(`verdict_mesh/e2e/${role}`),
        Buffer.from(treasury.secretKey.slice(0, 32)),
        Buffer.from([index]),
      ]),
    ),
  )
}

// ── PDA ─────────────────────────────────────────────────────────────────────

const pda = (seeds: (Buffer | Uint8Array)[], programId: PublicKey) =>
  PublicKey.findProgramAddressSync(seeds, programId)[0]

const u64 = (value: number | BN) => new BN(value).toArrayLike(Buffer, 'le', 8)

const escrowPda = (buyer: PublicKey, dealId: BN) =>
  pda([Buffer.from('escrow'), buyer.toBuffer(), u64(dealId)], referenceProgram.programId)
const escrowVaultPda = (escrow: PublicKey) =>
  pda([Buffer.from('escrow_vault'), escrow.toBuffer()], referenceProgram.programId)
const bondVaultPda = (escrow: PublicKey) =>
  pda([Buffer.from('bond_vault'), escrow.toBuffer()], referenceProgram.programId)
const epochPda = (network: PublicKey, epochId: BN) =>
  pda([Buffer.from('epoch'), network.toBuffer(), u64(epochId)], depinProgram.programId)
const epochVaultPda = (epoch: PublicKey) =>
  pda([Buffer.from('epoch_vault'), epoch.toBuffer()], depinProgram.programId)

// ── small helpers ───────────────────────────────────────────────────────────

/**
 * The verdict from the dispute state. Anchor returns an enum as an object with a
 * single key; `null` is impossible here — it never follows a successful
 * `tally` — but silently assuming so would make a run error look like a
 * protocol error.
 */
function outcomeOf(verdict: { claimant?: object; respondent?: object; statusQuo?: object } | null) {
  if (verdict === null) throw new Error('The dispute was tallied but carries no verdict')
  if (verdict.claimant !== undefined) return 'claimant' as const
  if (verdict.respondent !== undefined) return 'respondent' as const
  return 'statusQuo' as const
}

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, Math.max(0, ms)))

/**
 * Waits for a window to close by the **chain's** clock, not the local one.
 * Deadlines are stored in the dispute in cluster time, and on devnet that drifts
 * from wall time by seconds — enough for an instruction to arrive a beat early.
 */
async function waitForDeadline(deadline: number): Promise<void> {
  for (;;) {
    const slot = await connection.getSlot('confirmed')
    const now = (await connection.getBlockTime(slot)) ?? Math.floor(Date.now() / 1000)
    if (now > deadline) return
    await sleep((deadline - now + 1) * 1000)
  }
}

/**
 * Fees come from the chain, not from an assumed 5000 lamports per signature.
 *
 * They are read **after** the cycle, not right after each transaction:
 * `getTransaction` is the most expensive method needed here, and the public
 * devnet RPC answers it with 429 exactly when the cycle is racing to make a
 * window. The cost of measuring must not leak into the measurement.
 */
async function feesOf(signatures: readonly string[]): Promise<bigint> {
  let total = 0n

  for (const signature of signatures) {
    let fee: number | undefined

    for (let attempt = 0; attempt < 8 && fee === undefined; attempt += 1) {
      try {
        const tx = await connection.getTransaction(signature, {
          commitment: 'confirmed',
          maxSupportedTransactionVersion: 0,
        })
        fee = tx?.meta?.fee
      } catch {
        // A 429 or a network failure is a delay, not a result: the transaction
        // is already confirmed, and its fee is not going anywhere.
      }
      if (fee === undefined) await sleep(1000 * (attempt + 1))
    }

    if (fee === undefined) {
      throw new Error(`Confirmed transaction ${signature} never came back with its fee`)
    }
    total += BigInt(fee)
  }

  return total
}

async function fundWithSol(recipients: PublicKey[], sol: number): Promise<void> {
  const lamports = Math.round(sol * LAMPORTS_PER_SOL)
  const transaction = new Transaction()
  for (const recipient of recipients) {
    transaction.add(
      SystemProgram.transfer({ fromPubkey: treasury.publicKey, toPubkey: recipient, lamports }),
    )
  }
  await sendAndConfirmTransaction(connection, transaction, [treasury])
}

/**
 * Funding with the settlement asset. The treasury is also the mint authority,
 * so run participants receive funds by minting, not by transfer from a reserve
 * that could run dry mid-run.
 */
async function mintTo(owner: PublicKey, amount: BN): Promise<PublicKey> {
  const ata = getAssociatedTokenAddressSync(settlementMint, owner)
  const transaction = new Transaction().add(
    createAssociatedTokenAccountIdempotentInstruction(
      treasury.publicKey,
      ata,
      owner,
      settlementMint,
    ),
    createMintToInstruction(settlementMint, ata, treasury.publicKey, BigInt(amount.toString())),
  )
  await sendAndConfirmTransaction(connection, transaction, [treasury])
  return ata
}

// ── protocol setup ──────────────────────────────────────────────────────────

/**
 * `initialize` and `register_integrator` run once per cluster, so the run checks
 * for their accounts instead of assuming "they should be there". It registers
 * its own `Integrator` under its own derived authority — the run's policy must
 * neither depend on someone else's nor overwrite it.
 */
async function ensureProtocol(): Promise<PublicKey> {
  const config = configAddress()

  if ((await connection.getAccountInfo(config)) === null) {
    await mesh.methods
      .initialize(treasury.publicKey, treasury.publicKey)
      .accountsPartial({
        payer: treasury.publicKey,
        config,
        settlementMint,
        systemProgram: SystemProgram.programId,
      })
      .rpc()
    console.log(`initialized the protocol · config ${config.toBase58()}`)
  }

  // The second authority, not the first: the first `Integrator` was registered
  // with a threshold of 150, which this run's jurors no longer reach — slashing
  // took 10% from one of them, and there is no way to top up a stake. A policy
  // is immutable after registration (`FR-003`), so this needs a new integrator,
  // not an edit to the old one. The DePIN escrow is a protocol of its own, so
  // it registers under an authority of its own (the third).
  const authority = derivedKey('integrator', escrowKind === 'reference' ? 1 : 2)
  const integrator = integratorAddress(authority.publicKey)

  if ((await connection.getAccountInfo(integrator)) === null) {
    await fundWithSol([authority.publicKey], 0.02)
    await sendAndConfirmTransaction(
      connection,
      new Transaction().add(
        await client.registerIntegrator({
          authority: authority.publicKey,
          escrowProgram: escrowProgramId,
          policy: POLICY,
        }),
      ),
      [treasury, authority],
    )
    console.log(`registered the integrator ${integrator.toBase58()}`)
  }

  // The treasury receives the protocol's share of the hearing fee — its token
  // account must exist before the first settlement, or the whole finalization
  // fails.
  await mintTo(treasury.publicKey, new BN(0))

  return integrator
}

interface Juror {
  keypair: Keypair
  slot: number
}

/**
 * How much SOL a juror must hold for the whole run. Vote account rent is
 * ~0.0017 SOL per hearing and is **not refunded**, so the amount grows with the
 * number of disputes. Measured: a 20-dispute run funded by eye died on the
 * sixteenth, and it looked like a protocol error, not an empty wallet.
 */
const jurorSolNeeded = () => 0.01 + disputeCount * 0.0025

/**
 * The run's jurors. One already staked is recognised by its own account and is
 * not staked again: `stake` creates that account via `init`, so a second call
 * would be rejected anyway, and the run has to be repeatable.
 */
async function ensureJurors(): Promise<Juror[]> {
  const jurors: Juror[] = []

  for (let index = 0; index < POLICY.panelSize; index += 1) {
    const keypair = derivedKey('juror', index)
    const record = jurorAddress(keypair.publicKey)
    const existing = await mesh.account.juror.fetchNullable(record)

    if (existing !== null) {
      // SOL has to be topped up every time: the juror pays the vote account
      // rent and never gets it back — `VoteCommit` is closed nowhere (debt
      // T020). So a juror's cost is linear in the number of hearings, and a
      // twenty-dispute run costs them four times as much as a five-dispute one.
      const balance = await connection.getBalance(keypair.publicKey)
      const needed = Math.round(jurorSolNeeded() * LAMPORTS_PER_SOL)
      if (balance < needed) {
        await fundWithSol([keypair.publicKey], (needed - balance) / LAMPORTS_PER_SOL)
      }

      jurors.push({ keypair, slot: existing.index })
      console.log(`juror ${keypair.publicKey.toBase58()} already staked at slot ${existing.index}`)
      continue
    }

    await fundWithSol([keypair.publicKey], jurorSolNeeded())
    const jurorTokens = await mintTo(keypair.publicKey, POLICY.jurorStake)

    const registry = await mesh.account.jurorRegistry.fetch(registryAddress())
    const slot = registry.jurorCount

    await mesh.methods
      .stake(POLICY.jurorStake)
      .accountsPartial({
        juror: keypair.publicKey,
        config: configAddress(),
        settlementMint,
        registry: registryAddress(),
        jurorAccount: record,
        jurorIndex: jurorIndexAddress(slot),
        jurorTokens,
        stakeVault: stakeVaultAddress(),
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([keypair])
      .rpc()

    jurors.push({ keypair, slot })
    console.log(`staked juror ${keypair.publicKey.toBase58()} at slot ${slot}`)
  }

  return jurors
}

// ── the escrow under test ───────────────────────────────────────────────────

/**
 * One deal of one escrow, from locked funds to an executed verdict. The
 * dispute is opened with `prepared.accounts` spread as is: both escrows name
 * their VerdictMesh accounts the way `open_dispute` does, which is the SDK's
 * side of `SC-004`.
 */
interface EscrowDeal {
  claimant: Keypair
  open(prepared: PreparedDispute): Promise<string>
  settle(dispute: PublicKey): Promise<string>
  /** `null` if the parties' balances are what the verdict says, else what is off. */
  check(verdict: 'claimant' | 'respondent' | 'statusQuo'): Promise<string | null>
}

const tokenBalance = async (account: PublicKey) =>
  BigInt((await connection.getTokenAccountBalance(account, 'confirmed')).value.amount)

async function referenceDeal(index: number, integrator: PublicKey): Promise<EscrowDeal> {
  const buyer = Keypair.generate()
  const seller = Keypair.generate()
  const dealId = new BN(index)

  const escrow = escrowPda(buyer.publicKey, dealId)
  const total = MILESTONES.reduce((sum, amount) => sum.add(amount), new BN(0))
  const bonds = POLICY.deposit.muln(MILESTONES.length)

  // The buyer pays rent for the deal, its vault and the bond vault; the seller
  // for the dispute account and its vault. About 0.007 each; the rest is
  // headroom for fees. More generous funding just locks devnet SOL in wallets
  // nobody needs after the run.
  await fundWithSol([buyer.publicKey, seller.publicKey], 0.012)
  const buyerTokens = await mintTo(buyer.publicKey, total.add(bonds))
  // The seller puts up their bond and the hearing deposit: they open the dispute.
  const sellerTokens = await mintTo(seller.publicKey, bonds.add(POLICY.deposit))

  await referenceProgram.methods
    .createEscrow(dealId, MILESTONES)
    .accountsPartial({
      buyer: buyer.publicKey,
      seller: seller.publicKey,
      mint: settlementMint,
      integrator,
      config: configAddress(),
      settlementMint,
      escrow,
      buyerTokens,
      vault: escrowVaultPda(escrow),
      buyerBondTokens: buyerTokens,
      sellerBondTokens: sellerTokens,
      bondVault: bondVaultPda(escrow),
      tokenProgram: TOKEN_PROGRAM_ID,
      settlementTokenProgram: TOKEN_PROGRAM_ID,
      systemProgram: SystemProgram.programId,
    })
    .signers([buyer, seller])
    .rpc()

  return {
    claimant: seller,
    open: (prepared) =>
      referenceProgram.methods
        .disputeMilestone(DISPUTED_MILESTONE)
        .accountsPartial({ ...prepared.accounts, claimant: seller.publicKey, escrow })
        .postInstructions(prepared.postInstructions)
        .signers([seller])
        .rpc(),
    settle: (dispute) =>
      referenceProgram.methods
        .settleMilestone(DISPUTED_MILESTONE)
        .accountsPartial({
          escrow,
          dispute,
          mint: settlementMint,
          buyerTokens,
          sellerTokens,
          vault: escrowVaultPda(escrow),
          settlementMint,
          buyerBondTokens: buyerTokens,
          sellerBondTokens: sellerTokens,
          bondVault: bondVaultPda(escrow),
          tokenProgram: TOKEN_PROGRAM_ID,
          settlementTokenProgram: TOKEN_PROGRAM_ID,
        })
        .rpc(),
    // The milestone escrow's balances are proven by its mollusk tests; this
    // run measured it twenty times before `check` existed.
    check: async () => null,
  }
}

/** A share of 60 % reported, 90 % claimed — 30 of the 100 are in dispute. */
const EPOCH_REWARD = unit(100)
const REPORTED_BPS = 6_000
const CLAIMED_BPS = 9_000
/** Long enough for the opening transaction to land, short enough not to matter. */
const CHALLENGE_WINDOW = new BN(600)

async function depinDeal(index: number, integrator: PublicKey): Promise<EscrowDeal> {
  const network = Keypair.generate()
  const operator = Keypair.generate()
  const epochId = new BN(index)
  const epoch = epochPda(network.publicKey, epochId)
  const vault = epochVaultPda(epoch)

  // The network pays rent for the epoch and its vault, the operator for the
  // dispute account and its vault.
  await fundWithSol([network.publicKey, operator.publicKey], 0.012)
  const networkTokens = await mintTo(network.publicKey, EPOCH_REWARD)
  // The operator holds just the hearing deposit: they challenge the report.
  const operatorTokens = await mintTo(operator.publicKey, POLICY.deposit)

  await depinProgram.methods
    .fundEpoch(epochId, EPOCH_REWARD, CHALLENGE_WINDOW)
    .accountsPartial({
      network: network.publicKey,
      operator: operator.publicKey,
      integrator,
      config: configAddress(),
      mint: settlementMint,
      epoch,
      networkTokens,
      vault,
      tokenProgram: TOKEN_PROGRAM_ID,
      systemProgram: SystemProgram.programId,
    })
    .signers([network, operator])
    .rpc()
  await depinProgram.methods
    .reportUsage(REPORTED_BPS)
    .accountsPartial({ network: network.publicKey, epoch })
    .signers([network])
    .rpc()

  return {
    claimant: operator,
    open: (prepared) =>
      depinProgram.methods
        .disputeEpoch(CLAIMED_BPS)
        .accountsPartial({ ...prepared.accounts, operator: operator.publicKey, epoch })
        .postInstructions(prepared.postInstructions)
        .signers([operator])
        .rpc(),
    settle: (dispute) =>
      depinProgram.methods
        .settleDispute()
        .accountsPartial({
          epoch,
          dispute,
          mint: settlementMint,
          networkTokens,
          operatorTokens,
          vault,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .rpc(),
    // Both parties are paid whichever way it goes; the operator's deposit
    // comes back out of the network's share only when the operator won.
    check: async (verdict) => {
      const reward = BigInt(EPOCH_REWARD.toString())
      const deposit = BigInt(POLICY.deposit.toString())
      const bps = BigInt(verdict === 'claimant' ? CLAIMED_BPS : REPORTED_BPS)
      const share = (reward * bps) / 10_000n
      const reimbursed = verdict === 'claimant' ? deposit : 0n
      const expected = { operator: share + reimbursed, network: reward - share - reimbursed }
      const actual = {
        operator: await tokenBalance(operatorTokens),
        network: await tokenBalance(networkTokens),
      }
      return actual.operator === expected.operator && actual.network === expected.network
        ? null
        : `epoch ${epoch.toBase58()} paid ${actual.operator}/${actual.network}, expected ${expected.operator}/${expected.network}`
    },
  }
}

const drivers: Record<
  'reference' | 'depin',
  (index: number, integrator: PublicKey) => Promise<EscrowDeal>
> = {
  reference: referenceDeal,
  depin: depinDeal,
}

// ── one cycle ───────────────────────────────────────────────────────────────

async function runCycle(index: number, integrator: PublicKey, jurors: Juror[]): Promise<Run> {
  const interventions: string[] = []
  const signatures: string[] = []

  const deal = await drivers[escrowKind](index, integrator)

  // The cycle starts here: `SC-001` measures "dispute → vote → payout", not
  // locking the funds, which happens the same way with or without a hearing.
  const started = Date.now()

  const prepared = await client.prepareDispute({ integrator, claimant: deal.claimant.publicKey })
  const dispute = prepared.dispute

  // Opening the dispute and selecting the panel are one transaction. The panel
  // is fixed by the slot recorded at opening (`FR-006`), so there is no reason
  // to split them in time, and keeping a dispute without a panel is a risk.
  signatures.push(await deal.open(prepared))

  const opened = await mesh.account.dispute.fetch(dispute)
  const panel = opened.panel.map((wallet) => wallet.toBase58())

  // Ballots are assigned by panel seat, not by staking order: selection is
  // deterministic, but not identical to the registry.
  const seated = panel.map((wallet, seat) => {
    const juror = jurors.find((candidate) => candidate.keypair.publicKey.toBase58() === wallet)
    // The registry is shared across the whole protocol and may hold jurors
    // this run did not create. The run's policy keeps them out with the stake
    // threshold — but if one is seated anyway, that is not a script failure
    // but a hearing that will not finish without a human, and `SC-005` must
    // see it.
    if (juror === undefined) {
      throw new Error(`Panel seat ${seat} went to ${wallet}, a juror this run cannot vote for`)
    }
    return {
      juror,
      choice: BALLOTS[seat] ?? 'claimant',
      salt: derivedKey(`salt/${dispute.toBase58()}`, seat).secretKey.slice(0, 32),
    }
  })

  const outside = webSeat === null ? undefined : seated[webSeat]
  const scripted = seated.filter((member) => member !== outside)
  if (outside) {
    // One line, machine-readable: whoever drives the browser waits for it.
    console.log(
      `HANDOFF ${JSON.stringify({ dispute: dispute.toBase58(), juror: outside.juror.keypair.publicKey.toBase58(), seat: webSeat, commitDeadline: opened.commitDeadline.toNumber(), revealDeadline: opened.revealDeadline.toNumber() })}`,
    )
  }

  for (const member of scripted) {
    const signature = await mesh.methods
      .commitVote([
        ...commitmentOf(dispute, member.juror.keypair.publicKey, SIDE[member.choice], member.salt),
      ])
      .accountsPartial({
        juror: member.juror.keypair.publicKey,
        dispute,
        vote: voteAddress(dispute, member.juror.keypair.publicKey),
        systemProgram: SystemProgram.programId,
      })
      .signers([member.juror.keypair])
      .rpc()
    signatures.push(signature)
  }

  if (outside) {
    const sealed = await waitForOutsideVote(
      dispute,
      outside.juror.keypair.publicKey,
      'sealed',
      opened.commitDeadline.toNumber(),
    )
    if (!sealed) interventions.push(`web seat ${webSeat} did not seal a vote`)
  }

  await waitForDeadline(opened.commitDeadline.toNumber())

  for (const member of scripted) {
    const signature = await mesh.methods
      .revealVote(ballotArg(SIDE[member.choice]), [...member.salt])
      .accountsPartial({
        juror: member.juror.keypair.publicKey,
        dispute,
        vote: voteAddress(dispute, member.juror.keypair.publicKey),
      })
      .signers([member.juror.keypair])
      .rpc()
    signatures.push(signature)
  }

  if (outside) {
    const keypair = outside.juror.keypair
    // Leave the script enough of the window to open the vote itself.
    const openedOutside = await waitForOutsideVote(
      dispute,
      keypair.publicKey,
      'opened',
      opened.revealDeadline.toNumber() - 20,
    )
    const vote = await mesh.account.voteCommit.fetchNullable(
      voteAddress(dispute, keypair.publicKey),
    )
    if (!openedOutside && vote && vote.choice === null) {
      const salt = webSalt(keypair, dispute)
      const choice = (['claimant', 'respondent'] as const).find((candidate) =>
        Buffer.from(commitmentOf(dispute, keypair.publicKey, SIDE[candidate], salt)).equals(
          Buffer.from(vote.commitment),
        ),
      )
      if (choice === undefined)
        throw new Error('The web seat sealed a vote the web salt does not open')
      signatures.push(
        await mesh.methods
          .revealVote(ballotArg(SIDE[choice]), [...salt])
          .accountsPartial({
            juror: keypair.publicKey,
            dispute,
            vote: voteAddress(dispute, keypair.publicKey),
          })
          .signers([keypair])
          .rpc(),
      )
      interventions.push(`web seat ${webSeat} was opened by the script`)
    }
  }

  await waitForDeadline(opened.revealDeadline.toNumber())
  signatures.push(await mesh.methods.tally().accountsPartial({ dispute }).rpc())

  const tallied = await mesh.account.dispute.fetch(dispute)
  const verdict = outcomeOf(tallied.verdict)

  await waitForDeadline(tallied.appealDeadline.toNumber())

  const settlement = await deal.settle(dispute)
  signatures.push(settlement)

  // The payout is done — the `SC-001` cycle is over. Settling the panel's
  // stakes comes after and is not waited on: it is the protocol's internal
  // business, and keeping other people's funds locked until then would be
  // exactly what `settle_milestone` does not do.
  const elapsedMs = Date.now() - started
  const mismatch = await deal.check(verdict)
  if (mismatch !== null) interventions.push(mismatch)

  const settleStakes = await mesh.methods
    .settleStakes()
    .accountsPartial({
      dispute,
      crank: treasury.publicKey,
      config: configAddress(),
      settlementMint,
      disputeVault: disputeVaultAddress(dispute),
      stakeVault: stakeVaultAddress(),
      treasuryTokens: getAssociatedTokenAddressSync(settlementMint, treasury.publicKey),
      tokenProgram: TOKEN_PROGRAM_ID,
    })
    .remainingAccounts(
      seated.flatMap((member) => [
        { pubkey: jurorAddress(member.juror.keypair.publicKey), isSigner: false, isWritable: true },
        {
          pubkey: voteAddress(dispute, member.juror.keypair.publicKey),
          isSigner: false,
          isWritable: false,
        },
      ]),
    )
    .rpc()
  signatures.push(settleStakes)

  return {
    dispute: dispute.toBase58(),
    elapsedMs,
    feeLamports: await feesOf(signatures),
    outcome: verdict,
    expected: expectedOutcome(
      seated.map((member) => member.choice),
      { ...POLICY, commitWindow: 60, revealWindow: 60, appealWindow: 90 },
    ) as Run['outcome'],
    interventions,
  }
}

/**
 * The salt the web panel derives (`apps/web/src/lib/vote.ts` → `saltMessage`):
 * sha256 of the wallet's ed25519 signature of a fixed message. ed25519 is
 * deterministic, so the script holding the juror's key gets the very same
 * salt the browser used.
 */
function webSalt(keypair: Keypair, dispute: PublicKey): Buffer {
  const message = Buffer.from(
    `VerdictMesh vote secret\n\nSigning this does not move funds. It derives the secret that seals your vote in hearing ${dispute.toBase58()}.\n\nverdict_mesh/vote-salt/v1:${dispute.toBase58()}`,
  )
  const key = createPrivateKey({
    key: Buffer.concat([
      Buffer.from('302e020100300506032b657004220420', 'hex'),
      Buffer.from(keypair.secretKey.slice(0, 32)),
    ]),
    format: 'der',
    type: 'pkcs8',
  })
  return createHash('sha256')
    .update(sign(null, message, key))
    .digest()
}

/** Polls the outside seat's vote until it reaches `step` or `deadline` (unix seconds) passes. */
async function waitForOutsideVote(
  dispute: PublicKey,
  juror: PublicKey,
  step: 'sealed' | 'opened',
  deadline: number,
): Promise<boolean> {
  for (;;) {
    const vote = await mesh.account.voteCommit.fetchNullable(voteAddress(dispute, juror))
    if (vote && (step === 'sealed' || vote.choice !== null)) return true
    if (Date.now() / 1000 >= deadline) return false
    await new Promise((resolve) => setTimeout(resolve, 2_000))
  }
}

// ── run ─────────────────────────────────────────────────────────────────────

const mint = await getMint(connection, settlementMint)
if (mint.decimals !== DECIMALS) {
  throw new Error(
    `Settlement mint has ${mint.decimals} decimals, the demo policy assumes ${DECIMALS}`,
  )
}

console.log(
  `arbitration ${mesh.programId.toBase58()} · ${escrowKind} escrow ${escrowProgramId.toBase58()}`,
)
console.log(
  `${disputeCount} disputes in waves of ${batchSize} · floor ${cycleFloorSeconds({ ...POLICY, commitWindow: 60, revealWindow: 60, appealWindow: 90 })}s per cycle`,
)

const integrator = await ensureProtocol()
const jurors = await ensureJurors()

/**
 * RPC rejections that escaped the promise chain. `@solana/web3.js` surfaces some
 * transport errors through a callback nobody awaits, and Node kills the process
 * mid-measurement. They cannot be swallowed silently: this is exactly what
 * `SC-005` calls "a hand was needed", so they are charged to the wave during
 * which they happened.
 */
const stray: string[] = []
process.on('unhandledRejection', (reason) => {
  stray.push(`stray RPC failure: ${reason}`)
  console.error(`stray rejection: ${reason}`)
})

const runs: Run[] = []
for (let start = 0; start < disputeCount; start += batchSize) {
  const wave = Array.from(
    { length: Math.min(batchSize, disputeCount - start) },
    (_, offset) => start + offset,
  )

  // A wave's cycles are staggered: a simultaneous start drops a dozen
  // transactions on the RPC at once, and the answer is 429 and "blockhash not
  // found" — a failed measurement for a reason unrelated to the protocol.
  const results = await Promise.allSettled(
    wave.map(async (index, offset) => {
      await sleep(offset * 3000)
      return runCycle(index, integrator, jurors)
    }),
  )

  // Whatever failed outside the promises belongs to this wave — charge it to
  // the wave's first cycle, since it cannot be pinned down more precisely and
  // must not be lost.
  const strayHere = stray.splice(0)

  results.forEach((result, offset) => {
    if (result.status === 'fulfilled') {
      const run = result.value
      console.log(
        `dispute ${wave[offset]} · ${(run.elapsedMs / 1000).toFixed(1)}s · ${run.feeLamports} lamports · ${run.outcome}`,
      )
      runs.push(offset === 0 ? { ...run, interventions: strayHere } : run)
      return
    }

    // A failed cycle is not a missing result but the most important one:
    // `SC-005` measures exactly the share of hearings carried through without a
    // human.
    console.error(`dispute ${wave[offset]} needed a hand: ${result.reason}`)
    runs.push({
      dispute: `deal ${wave[offset]}`,
      elapsedMs: Number.POSITIVE_INFINITY,
      feeLamports: 0n,
      outcome: 'statusQuo',
      expected: 'statusQuo',
      interventions: [`deal ${wave[offset]}: ${result.reason}`, ...(offset === 0 ? strayHere : [])],
    })
  })
}

const report = measure(runs, { cycleSeconds: 300, cycleUsd: 0.1, solPriceUsd })

console.log('')
console.log(
  `SC-001 full cycle      ${report.sc001.passed ? 'PASS' : 'FAIL'} · slowest ${(report.sc001.slowestMs / 1000).toFixed(1)}s of ${report.sc001.budgetMs / 1000}s`,
)
console.log(
  `SC-002 fees per cycle  ${report.sc002.passed ? 'PASS' : 'FAIL'} · worst $${report.sc002.worstUsd.toFixed(4)} of $${report.sc002.budgetUsd} at $${solPriceUsd}/SOL`,
)
console.log(
  `SC-005 no hands needed ${report.sc005.passed ? 'PASS' : 'FAIL'} · ${report.sc005.automatic} of ${report.sc005.total} automatic`,
)
for (const line of report.sc005.needed) console.log(`  ${line}`)

process.exit(report.sc001.passed && report.sc002.passed && report.sc005.passed ? 0 : 1)
