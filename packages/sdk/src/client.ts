import { type IdlTypes, Program, type Provider } from '@coral-xyz/anchor'
import { getAssociatedTokenAddressSync } from '@solana/spl-token'
import {
  type AccountMeta,
  type PublicKey,
  SystemProgram,
  type TransactionInstruction,
} from '@solana/web3.js'
import type { VerdictMesh } from './idl/verdict_mesh.js'
import { verdictMeshIdl } from './idl/verdict-mesh.js'
import {
  configAddress,
  disputeAddress,
  disputeVaultAddress,
  integratorAddress,
  jurorAddress,
  jurorIndexAddress,
  PROGRAM_ID,
  registryAddress,
} from './pda.js'

/** An integrator's arbitration policy, as `register_integrator` takes it (`FR-002`). */
export type Policy = IdlTypes<VerdictMesh>['policy']

/**
 * The VerdictMesh accounts an escrow passes on when it opens a dispute through
 * CPI. The names are those of `open_dispute`, with `claimantTokens` for its
 * `depositor_tokens`: an escrow that names its own accounts the same way takes
 * this object as is (`.accountsPartial({ ...prepared.accounts, escrow, claimant })`).
 */
export interface DisputeAccounts {
  integrator: PublicKey
  config: PublicKey
  settlementMint: PublicKey
  dispute: PublicKey
  /** The claimant's associated account for the settlement mint: the deposit comes from it. */
  claimantTokens: PublicKey
  disputeVault: PublicKey
  verdictMeshProgram: PublicKey
  tokenProgram: PublicKey
  systemProgram: PublicKey
}

export interface PreparedDispute {
  /** The dispute that the escrow's instruction will open. */
  dispute: PublicKey
  /** Its number within the integrator. */
  id: bigint
  accounts: DisputeAccounts
  /** Panel selection, to go into the same transaction right after the escrow's instruction. */
  postInstructions: TransactionInstruction[]
}

/**
 * The integrator's side of VerdictMesh: registering a protocol with its policy
 * and opening disputes from an escrow.
 *
 * A dispute is opened by the integrator's own escrow program, through CPI into
 * `open_dispute`, so the SDK does not build that instruction — the escrow
 * does. What the SDK supplies is everything around it: the VerdictMesh
 * accounts the escrow has to pass on, and the panel selection that has to
 * follow in the same transaction (`FR-006`). `program` is the plain Anchor
 * client for anything else.
 */
export class VerdictMeshClient {
  readonly program: Program<VerdictMesh>

  constructor(provider: Provider, programId: PublicKey = PROGRAM_ID) {
    // The IDL value is snake_case, the way `anchor build` writes it, and its
    // type is the camelCase one Anchor generates next to it; `Program`
    // converts the first into the second itself, so the two only meet here.
    const idl = { ...verdictMeshIdl, address: programId.toBase58() } as unknown as VerdictMesh
    this.program = new Program<VerdictMesh>(idl, provider)
  }

  get programId(): PublicKey {
    return this.program.programId
  }

  /**
   * Registers a protocol: `authority` signs and pays, `escrowProgram` is the
   * only program that will be able to open disputes under this integrator.
   * The policy is checked by the program, not here — an invalid one fails the
   * transaction with `InvalidPolicy`, and the program log names the rule.
   * The policy cannot be changed afterwards (`FR-003`).
   */
  registerIntegrator(input: {
    authority: PublicKey
    escrowProgram: PublicKey
    policy: Policy
  }): Promise<TransactionInstruction> {
    return this.program.methods
      .registerIntegrator(input.policy)
      .accountsPartial({
        authority: input.authority,
        integrator: integratorAddress(input.authority, this.programId),
        config: configAddress(this.programId),
        escrowProgram: input.escrowProgram,
        systemProgram: SystemProgram.programId,
      })
      .instruction()
  }

  /**
   * Everything an escrow needs to open a dispute under `integrator` (the
   * integrator's address, not its authority) on behalf of `claimant`.
   *
   * The claimant signs the transaction too: they pay the review deposit
   * (`FR-026`), and a dispute opened in someone's name without their signature
   * is refused by design.
   *
   * The dispute's address comes from the integrator's dispute counter as it is
   * now. If another dispute of the same integrator lands first, this
   * transaction fails on the dispute's seeds and nothing is opened — prepare
   * again and resend.
   */
  async prepareDispute(input: {
    integrator: PublicKey
    claimant: PublicKey
  }): Promise<PreparedDispute> {
    const [integrator, config] = await Promise.all([
      this.program.account.integrator.fetch(input.integrator),
      this.program.account.config.fetch(configAddress(this.programId)),
    ])
    const settlementMint = config.settlementMint
    const tokenProgram = await this.tokenProgramOf(settlementMint)

    const id = BigInt(integrator.disputeCount.toString())
    const dispute = disputeAddress(input.integrator, id, this.programId)

    return {
      dispute,
      id,
      accounts: {
        integrator: input.integrator,
        config: configAddress(this.programId),
        settlementMint,
        dispute,
        claimantTokens: getAssociatedTokenAddressSync(
          settlementMint,
          input.claimant,
          true,
          tokenProgram,
        ),
        disputeVault: disputeVaultAddress(dispute, this.programId),
        verdictMeshProgram: this.programId,
        tokenProgram,
        systemProgram: SystemProgram.programId,
      },
      postInstructions: [await this.selectPanelInstruction(dispute)],
    }
  }

  /**
   * Draws the panel of `dispute`. Permissionless, and the draw is fixed by
   * the slot recorded when the dispute opened, so sending it later cannot
   * change who sits — but a dispute that never gets a panel escalates
   * empty-handed and ends in `StatusQuo`.
   *
   * It carries the whole juror registry, two accounts per juror. With a large
   * registry the instruction no longer fits into one transaction next to the
   * escrow's: then send it on its own, right after the opening.
   */
  async selectPanelInstruction(dispute: PublicKey): Promise<TransactionInstruction> {
    return this.program.methods
      .selectPanel()
      .accountsPartial({ dispute, registry: registryAddress(this.programId) })
      .remainingAccounts(await this.registryAccounts())
      .instruction()
  }

  /** The registry as the selection reads it: (`JurorIndex`, `Juror`) for every slot, in order. */
  async registryAccounts(): Promise<AccountMeta[]> {
    const registry = await this.program.account.jurorRegistry.fetch(registryAddress(this.programId))
    const slots = Array.from({ length: registry.jurorCount }, (_, slot) =>
      jurorIndexAddress(slot, this.programId),
    )
    const entries = await this.program.account.jurorIndex.fetchMultiple(slots)

    return slots.flatMap((slot, i) => {
      const entry = entries[i]
      // The registry counts this slot, so its account exists; a gap means a
      // read from a lagging RPC node, and a selection built on it would fail.
      if (!entry) throw new Error(`Registry slot ${i} (${slot.toBase58()}) could not be read`)
      return [
        { pubkey: slot, isSigner: false, isWritable: false },
        { pubkey: jurorAddress(entry.wallet, this.programId), isSigner: false, isWritable: true },
      ]
    })
  }

  /** The settlement mint can be a classic SPL token or a Token-2022 one; its owner says which. */
  private async tokenProgramOf(mint: PublicKey): Promise<PublicKey> {
    const info = await this.program.provider.connection.getAccountInfo(mint)
    if (!info) throw new Error(`Settlement mint ${mint.toBase58()} does not exist on this cluster`)
    return info.owner
  }
}
