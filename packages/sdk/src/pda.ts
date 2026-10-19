import { PublicKey } from '@solana/web3.js'

/**
 * Addresses of the VerdictMesh accounts. Everything here derives from seeds,
 * so nothing has to be fetched — and nothing here imports Anchor: a page that
 * only needs an address does not have to carry the coder.
 *
 * The seeds are `programs/verdict-mesh/src/seeds.rs`. A mismatch between two
 * copies of a seed is not an error anywhere; it is a PDA with nothing behind
 * it, which is why the tests pin these against addresses seen on devnet.
 */

/** The program on devnet. Every function takes another one for a local validator. */
export const PROGRAM_ID = new PublicKey('8WyWpDD1ZbkTRGG6SRcYyWxApPsHaSgWn2SWJQ8xSgxq')

const encode = (text: string) => new TextEncoder().encode(text)

export const SEEDS = {
  config: encode('config'),
  integrator: encode('integrator'),
  registry: encode('registry'),
  juror: encode('juror'),
  jurorIndex: encode('juror_idx'),
  dispute: encode('dispute'),
  vote: encode('vote'),
  stakeVault: encode('stake_vault'),
  disputeVault: encode('dispute_vault'),
} as const

const find = (seeds: Uint8Array[], programId: PublicKey) =>
  PublicKey.findProgramAddressSync(seeds, programId)[0]

/** Little-endian, the way the program writes `u64` and `u32` into seeds. */
const le = (value: bigint | number, bytes: 4 | 8) => {
  const out = new Uint8Array(bytes)
  const view = new DataView(out.buffer)
  if (bytes === 8) view.setBigUint64(0, BigInt(value), true)
  else view.setUint32(0, Number(value), true)
  return out
}

/** Protocol settings: the settlement mint, the reporter, the treasury. One per program. */
export const configAddress = (programId = PROGRAM_ID) => find([SEEDS.config], programId)

/** The juror registry. One per program. */
export const registryAddress = (programId = PROGRAM_ID) => find([SEEDS.registry], programId)

/** Where juror stakes are held. One per program. */
export const stakeVaultAddress = (programId = PROGRAM_ID) => find([SEEDS.stakeVault], programId)

/**
 * An integrator's record, keyed by the wallet that registered it. This is the
 * address an escrow stores and passes on: disputes are numbered per integrator.
 */
export const integratorAddress = (authority: PublicKey, programId = PROGRAM_ID) =>
  find([SEEDS.integrator, authority.toBytes()], programId)

/** Dispute number `id` of an integrator — `Integrator.dispute_count` at the moment it opened. */
export const disputeAddress = (
  integrator: PublicKey,
  id: bigint | number,
  programId = PROGRAM_ID,
) => find([SEEDS.dispute, integrator.toBytes(), le(id, 8)], programId)

/** Holds a dispute's review deposit until the panel is paid. */
export const disputeVaultAddress = (dispute: PublicKey, programId = PROGRAM_ID) =>
  find([SEEDS.disputeVault, dispute.toBytes()], programId)

/** A juror's record, keyed by the juror's wallet. */
export const jurorAddress = (wallet: PublicKey, programId = PROGRAM_ID) =>
  find([SEEDS.juror, wallet.toBytes()], programId)

/** Slot `index` of the registry: which juror sits there. */
export const jurorIndexAddress = (index: number, programId = PROGRAM_ID) =>
  find([SEEDS.jurorIndex, le(index, 4)], programId)

/** One juror's vote in one dispute. `juror` is the wallet, not the juror record. */
export const voteAddress = (dispute: PublicKey, juror: PublicKey, programId = PROGRAM_ID) =>
  find([SEEDS.vote, dispute.toBytes(), juror.toBytes()], programId)
