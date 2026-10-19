import { AnchorProvider, BN, BorshInstructionCoder, Wallet } from '@coral-xyz/anchor'
import { getAssociatedTokenAddressSync, TOKEN_2022_PROGRAM_ID } from '@solana/spl-token'
import {
  type AccountInfo,
  type Connection,
  Keypair,
  PublicKey,
  SYSVAR_SLOT_HASHES_PUBKEY,
  SystemProgram,
} from '@solana/web3.js'
import { describe, expect, it } from 'vitest'
import { type Policy, VerdictMeshClient } from './client.js'
import {
  configAddress,
  disputeVaultAddress,
  integratorAddress,
  jurorAddress,
  jurorIndexAddress,
  PROGRAM_ID,
  registryAddress,
} from './pda.js'

/**
 * The client reads four accounts and builds two instructions; a cluster is
 * not needed for either. The accounts here are encoded with the program's own
 * coder, so what the client reads back is what the program would have written.
 */
class Chain {
  readonly accounts = new Map<string, AccountInfo<Buffer>>()

  put(address: PublicKey, owner: PublicKey, data: Buffer) {
    this.accounts.set(address.toBase58(), {
      data,
      owner,
      executable: false,
      lamports: 1_000_000,
      rentEpoch: 0,
    })
  }

  private read = (address: PublicKey) => this.accounts.get(address.toBase58()) ?? null

  readonly connection = {
    getAccountInfo: async (address: PublicKey) => this.read(address),
    getAccountInfoAndContext: async (address: PublicKey) => ({
      context: { slot: 1 },
      value: this.read(address),
    }),
    getMultipleAccountsInfo: async (addresses: PublicKey[]) => addresses.map(this.read),
    getMultipleAccountsInfoAndContext: async (addresses: PublicKey[]) => ({
      context: { slot: 1 },
      value: addresses.map(this.read),
    }),
  }
}

const policy: Policy = {
  panelSize: 3,
  extendedPanelSize: 5,
  quorum: 2,
  extendedQuorum: 3,
  jurorStake: new BN(120_000_000),
  slashBpsWrong: 1_000,
  slashBpsNoReveal: 2_000,
  commitWindow: new BN(60),
  revealWindow: new BN(60),
  appealWindow: new BN(90),
  optimisticWindow: new BN(60),
  deposit: new BN(5_000_000),
  optimisticThreshold: new BN(50_000_000),
}

/** The devnet dispute from `pda.test.ts`: number 106 of this integrator. */
const integrator = new PublicKey('DezHTPxw6bX5Dna6fXCQFmzBuk8Ud435SLa8Em93XbZ7')
const devnetDispute = new PublicKey('9qVtSBKgvNWizi5TdTier37WLNnrmdyqJ8vUnGoRLKMP')

function setup(options: { jurors: number; skipSlot?: number }) {
  const chain = new Chain()
  const provider = new AnchorProvider(
    chain.connection as unknown as Connection,
    new Wallet(Keypair.generate()),
    {},
  )
  const client = new VerdictMeshClient(provider)
  const coder = client.program.coder.accounts
  const mint = PublicKey.unique()
  const wallets = Array.from({ length: options.jurors }, () => PublicKey.unique())

  const put = async (address: PublicKey, name: string, value: object) =>
    chain.put(address, PROGRAM_ID, await coder.encode(name, value))

  return {
    chain,
    client,
    mint,
    wallets,
    async seed() {
      await put(configAddress(), 'config', {
        settlementMint: mint,
        reporter: PublicKey.unique(),
        treasury: PublicKey.unique(),
        bump: 255,
      })
      await put(integrator, 'integrator', {
        authority: PublicKey.unique(),
        escrowProgram: PublicKey.unique(),
        policy,
        disputeCount: new BN(106),
        bump: 255,
      })
      await put(registryAddress(), 'jurorRegistry', { jurorCount: options.jurors, bump: 255 })
      for (const [slot, wallet] of wallets.entries()) {
        if (slot === options.skipSlot) continue
        await put(jurorIndexAddress(slot), 'jurorIndex', { wallet, bump: 255 })
      }
      // A Token-2022 settlement mint: the token program has to come from the mint's owner.
      chain.put(mint, TOKEN_2022_PROGRAM_ID, Buffer.alloc(82))
    },
  }
}

describe('registerIntegrator', () => {
  it('carries the policy and the accounts the program checks', async () => {
    const { client } = setup({ jurors: 0 })
    const authority = PublicKey.unique()
    const escrowProgram = PublicKey.unique()

    const ix = await client.registerIntegrator({ authority, escrowProgram, policy })
    const decoded = new BorshInstructionCoder(client.program.idl).decode(ix.data)

    expect(ix.programId.equals(PROGRAM_ID)).toBe(true)
    expect(decoded?.name).toBe('registerIntegrator')
    expect(JSON.stringify(decoded?.data)).toBe(JSON.stringify({ policy }))
    expect(ix.keys.map((key) => key.pubkey.toBase58())).toEqual(
      [
        authority,
        integratorAddress(authority),
        configAddress(),
        escrowProgram,
        SystemProgram.programId,
      ].map((key) => key.toBase58()),
    )
    expect(ix.keys[0]?.isSigner).toBe(true)
  })
})

describe('prepareDispute', () => {
  it('names the dispute the integrator will open next', async () => {
    const setupResult = setup({ jurors: 2 })
    await setupResult.seed()

    const prepared = await setupResult.client.prepareDispute({
      integrator,
      claimant: PublicKey.unique(),
    })

    expect(prepared.id).toBe(106n)
    expect(prepared.dispute.equals(devnetDispute)).toBe(true)
  })

  it('gives the escrow every VerdictMesh account of open_dispute', async () => {
    const { client, mint, seed } = setup({ jurors: 2 })
    await seed()
    const claimant = PublicKey.unique()

    const { accounts } = await client.prepareDispute({ integrator, claimant })

    expect(accounts).toEqual({
      integrator,
      config: configAddress(),
      settlementMint: mint,
      dispute: devnetDispute,
      claimantTokens: getAssociatedTokenAddressSync(mint, claimant, true, TOKEN_2022_PROGRAM_ID),
      disputeVault: disputeVaultAddress(devnetDispute),
      verdictMeshProgram: PROGRAM_ID,
      tokenProgram: TOKEN_2022_PROGRAM_ID,
      systemProgram: SystemProgram.programId,
    })
  })

  it('follows with the panel draw over the whole registry, in slot order', async () => {
    const { client, wallets, seed } = setup({ jurors: 3 })
    await seed()

    const { postInstructions } = await client.prepareDispute({
      integrator,
      claimant: PublicKey.unique(),
    })
    const [select] = postInstructions

    expect(postInstructions).toHaveLength(1)
    expect(
      new BorshInstructionCoder(client.program.idl).decode(select?.data ?? Buffer.alloc(0))?.name,
    ).toBe('selectPanel')
    expect(select?.keys.map((key) => [key.pubkey.toBase58(), key.isWritable])).toEqual([
      [devnetDispute.toBase58(), true],
      [registryAddress().toBase58(), false],
      [SYSVAR_SLOT_HASHES_PUBKEY.toBase58(), false],
      ...wallets.flatMap((wallet, slot) => [
        [jurorIndexAddress(slot).toBase58(), false],
        [jurorAddress(wallet).toBase58(), true],
      ]),
    ])
  })

  it('refuses to draw from a registry it could not read whole', async () => {
    const { client, seed } = setup({ jurors: 3, skipSlot: 1 })
    await seed()

    await expect(
      client.prepareDispute({ integrator, claimant: PublicKey.unique() }),
    ).rejects.toThrow(/Registry slot 1/)
  })

  it('says so when the settlement mint is missing', async () => {
    const { client, chain, mint, seed } = setup({ jurors: 1 })
    await seed()
    chain.accounts.delete(mint.toBase58())

    await expect(
      client.prepareDispute({ integrator, claimant: PublicKey.unique() }),
    ).rejects.toThrow(/does not exist/)
  })
})

describe('another program id', () => {
  it('derives every address under it', async () => {
    const local = PublicKey.unique()
    const chain = new Chain()
    const provider = new AnchorProvider(
      chain.connection as unknown as Connection,
      new Wallet(Keypair.generate()),
      {},
    )
    const client = new VerdictMeshClient(provider, local)
    const authority = PublicKey.unique()

    const ix = await client.registerIntegrator({
      authority,
      escrowProgram: PublicKey.unique(),
      policy,
    })

    expect(ix.programId.equals(local)).toBe(true)
    expect(ix.keys[1]?.pubkey.equals(integratorAddress(authority, local))).toBe(true)
  })
})
