import { keccak_256 } from '@noble/hashes/sha3'
import { sha256 } from '@noble/hashes/sha256'
import { PublicKey, SystemProgram, TransactionInstruction } from '@solana/web3.js'
import { Buffer } from 'buffer'

/**
 * A juror's vote, built in the browser and sent straight to the program from
 * the juror's wallet (`FR-014`: the service has no write API to send it
 * through).
 *
 * Nothing here is generated from the IDL at runtime: the two instructions and
 * two accounts the panel needs are small, and carrying Anchor's coder into the
 * page for them would cost more than it checks. What keeps these bytes honest
 * is the program itself — a commitment that does not match is a reveal the
 * program refuses — and the tests against the formula the devnet runs used.
 */

/** `programs/verdict-mesh/src/vote.rs` → `DOMAIN`. */
const DOMAIN = new TextEncoder().encode('verdict_mesh/vote/v1')

export type Ballot = 'Claimant' | 'Respondent'

/** `vote.rs` → `tag`: numbered from one, so zeroed memory is never a vote. */
const TAG: Record<Ballot, number> = { Claimant: 1, Respondent: 2 }

/** The borsh index of `Ballot` in the instruction argument — declaration order. */
const VARIANT: Record<Ballot, number> = { Claimant: 0, Respondent: 1 }

export const SALT_LEN = 32

const concat = (...parts: Uint8Array[]) => {
  const out = new Uint8Array(parts.reduce((n, part) => n + part.length, 0))
  let offset = 0
  for (const part of parts) {
    out.set(part, offset)
    offset += part.length
  }
  return out
}

/** `vote.rs` → `commitment_of`. The one formula; there is no other copy to drift from. */
export function commitmentOf(
  dispute: PublicKey,
  juror: PublicKey,
  choice: Ballot,
  salt: Uint8Array,
): Uint8Array {
  if (salt.length !== SALT_LEN) throw new Error(`The salt must be ${SALT_LEN} bytes`)
  return keccak_256(
    concat(DOMAIN, dispute.toBytes(), juror.toBytes(), Uint8Array.of(TAG[choice]), salt),
  )
}

/**
 * What the wallet signs to produce the salt of one dispute. ed25519 signatures
 * are deterministic, so the same wallet gets the same salt on any device, at
 * any time — nothing has to be stored for the reveal, and a cleared browser
 * does not cost a juror their stake. The text says what it is for, because a
 * wallet shows it to the person signing.
 */
export const saltMessage = (dispute: PublicKey) =>
  new TextEncoder().encode(
    `VerdictMesh vote secret\n\nSigning this does not move funds. It derives the secret that seals your vote in hearing ${dispute.toBase58()}.\n\nverdict_mesh/vote-salt/v1:${dispute.toBase58()}`,
  )

/** The salt from that signature. Hashed, so the signature itself never goes on chain. */
export const saltFromSignature = (signature: Uint8Array) => sha256(signature)

/** A random salt, for a wallet that cannot sign messages. Must be kept until the reveal. */
export const randomSalt = () => crypto.getRandomValues(new Uint8Array(SALT_LEN))

const SEED = {
  juror: new TextEncoder().encode('juror'),
  vote: new TextEncoder().encode('vote'),
}

export const jurorAddress = (programId: PublicKey, wallet: PublicKey) =>
  PublicKey.findProgramAddressSync([SEED.juror, wallet.toBytes()], programId)[0]

export const voteAddress = (programId: PublicKey, dispute: PublicKey, juror: PublicKey) =>
  PublicKey.findProgramAddressSync([SEED.vote, dispute.toBytes(), juror.toBytes()], programId)[0]

/** Anchor discriminators, from `target/idl/verdict_mesh.json`. */
const DISCRIMINATOR = {
  commitVote: Uint8Array.of(134, 97, 90, 126, 91, 66, 16, 26),
  revealVote: Uint8Array.of(100, 157, 139, 17, 186, 75, 185, 149),
  juror: Uint8Array.of(209, 201, 239, 217, 237, 84, 189, 152),
  voteCommit: Uint8Array.of(125, 216, 109, 1, 40, 87, 250, 47),
}

export function commitVoteInstruction(
  programId: PublicKey,
  dispute: PublicKey,
  juror: PublicKey,
  commitment: Uint8Array,
) {
  return new TransactionInstruction({
    programId,
    keys: [
      { pubkey: juror, isSigner: true, isWritable: true },
      { pubkey: dispute, isSigner: false, isWritable: false },
      { pubkey: voteAddress(programId, dispute, juror), isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    data: Buffer.from(concat(DISCRIMINATOR.commitVote, commitment)),
  })
}

export function revealVoteInstruction(
  programId: PublicKey,
  dispute: PublicKey,
  juror: PublicKey,
  choice: Ballot,
  salt: Uint8Array,
) {
  return new TransactionInstruction({
    programId,
    keys: [
      { pubkey: juror, isSigner: true, isWritable: false },
      { pubkey: dispute, isSigner: false, isWritable: true },
      { pubkey: voteAddress(programId, dispute, juror), isSigner: false, isWritable: true },
    ],
    data: Buffer.from(concat(DISCRIMINATOR.revealVote, Uint8Array.of(VARIANT[choice]), salt)),
  })
}

const startsWith = (data: Uint8Array, prefix: Uint8Array) =>
  data.length >= prefix.length && prefix.every((byte, i) => data[i] === byte)

const u64 = (data: Uint8Array, offset: number) =>
  new DataView(data.buffer, data.byteOffset + offset, 8).getBigUint64(0, true)

const u16 = (data: Uint8Array, offset: number) =>
  new DataView(data.buffer, data.byteOffset + offset, 2).getUint16(0, true)

export interface JurorAccount {
  stake: bigint
  activeDisputes: number
}

/** `Juror`: discriminator, `wallet`, `stake: u64`, `active_disputes: u16`, … */
export function decodeJuror(data: Uint8Array): JurorAccount | null {
  if (!startsWith(data, DISCRIMINATOR.juror) || data.length < 8 + 32 + 8 + 2) return null
  return { stake: u64(data, 40), activeDisputes: u16(data, 48) }
}

export interface VoteCommitAccount {
  commitment: Uint8Array
  /** `null` — sealed, not revealed yet. */
  choice: Ballot | null
  round: number
}

/** `VoteCommit`: discriminator, `dispute`, `juror`, `commitment`, `choice: Option<Ballot>`, `round`. */
export function decodeVoteCommit(data: Uint8Array): VoteCommitAccount | null {
  if (!startsWith(data, DISCRIMINATOR.voteCommit) || data.length < 8 + 32 + 32 + 32 + 1) {
    return null
  }
  const commitment = data.slice(72, 104)
  const tag = data[104]
  let choice: Ballot | null
  let next: number
  if (tag === 0) {
    choice = null
    next = 105
  } else if (tag === 1 && data[105] !== undefined) {
    choice = data[105] === 0 ? 'Claimant' : 'Respondent'
    next = 106
  } else {
    return null
  }
  return { commitment, choice, round: data[next] ?? 0 }
}

/**
 * Where this juror stands in a dispute, from the vote account as it is on chain.
 *
 * - `not-seated` — the wallet is not on the panel; the program would refuse it.
 * - `can-commit` — seated, nothing sealed yet.
 * - `committed` — sealed this round, not yet opened.
 * - `revealed` — opened; it counts.
 * - `stale` — sealed in the first round of an escalated dispute. The program
 *   creates the vote account once (`init`), so it can be neither opened in the
 *   second round nor sealed again.
 */
export type VoteStep = 'not-seated' | 'can-commit' | 'committed' | 'revealed' | 'stale'

export function voteStep(
  dispute: { panel: readonly string[]; escalated: boolean },
  wallet: string,
  vote: VoteCommitAccount | null,
): VoteStep {
  if (!dispute.panel.includes(wallet)) return 'not-seated'
  if (vote === null) return 'can-commit'
  // `Dispute::round` is `u8::from(escalated)`.
  if (vote.round !== (dispute.escalated ? 1 : 0)) return 'stale'
  return vote.choice === null ? 'committed' : 'revealed'
}

/**
 * Which choice a salt opens, read back from the commitment on chain. Two
 * choices, so trying both is cheaper than remembering one — and nothing kept
 * in the browser can be lost or tampered with. `null` — this salt opens
 * neither: the vote was sealed with another secret.
 */
export function choiceSealedBy(
  dispute: PublicKey,
  juror: PublicKey,
  salt: Uint8Array,
  commitment: Uint8Array,
): Ballot | null {
  const same = (a: Uint8Array, b: Uint8Array) =>
    a.length === b.length && a.every((byte, i) => byte === b[i])
  for (const choice of ['Claimant', 'Respondent'] as const) {
    if (same(commitmentOf(dispute, juror, choice, salt), commitment)) return choice
  }
  return null
}

const SALT_KEY = (dispute: PublicKey, juror: PublicKey) =>
  `verdictmesh.salt.${dispute.toBase58()}.${juror.toBase58()}`

/**
 * The fallback for a wallet that cannot sign messages: a random salt, kept in
 * this browser until the reveal. Written before the commit is sent, so a
 * crash between the two cannot leave a sealed vote nobody can open.
 */
export function storeSalt(dispute: PublicKey, juror: PublicKey, salt: Uint8Array): boolean {
  try {
    localStorage.setItem(SALT_KEY(dispute, juror), Buffer.from(salt).toString('hex'))
    return localStorage.getItem(SALT_KEY(dispute, juror)) !== null
  } catch {
    return false
  }
}

export function storedSalt(dispute: PublicKey, juror: PublicKey): Uint8Array | null {
  try {
    const value = localStorage.getItem(SALT_KEY(dispute, juror))
    return value === null ? null : Uint8Array.from(Buffer.from(value, 'hex'))
  } catch {
    return null
  }
}
