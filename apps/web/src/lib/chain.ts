import { Connection, PublicKey, type Transaction } from '@solana/web3.js'
import {
  decodeJuror,
  decodeVoteCommit,
  type JurorAccount,
  jurorAddress,
  type VoteCommitAccount,
  voteAddress,
} from './vote'

/**
 * The page's own line to the chain — for what only the juror's wallet may do
 * (`FR-014`) and for the juror's own accounts. Everything about the dispute
 * itself comes from `api`.
 *
 * The RPC key, if any, is public by construction: it ships in the bundle. It
 * is a key of its own, restricted to the site's domain, never the one the
 * service runs on.
 */

export const RPC_URL: string =
  import.meta.env.VITE_SOLANA_RPC_URL ?? 'https://api.devnet.solana.com'

export const PROGRAM_ID = new PublicKey(
  import.meta.env.VITE_VERDICT_MESH_PROGRAM_ID ?? '8WyWpDD1ZbkTRGG6SRcYyWxApPsHaSgWn2SWJQ8xSgxq',
)

/** Wallet Standard chain id of this network. */
export const CHAIN = 'solana:devnet'

/**
 * `confirmed`, not the web3.js default: without a commitment it reads
 * `finalized`, a dozen seconds behind, and a juror who just sealed a vote
 * would be told nothing is sealed.
 */
export const connection = new Connection(RPC_URL, 'confirmed')

export async function readJuror(wallet: PublicKey): Promise<JurorAccount | null> {
  const account = await connection.getAccountInfo(jurorAddress(PROGRAM_ID, wallet))
  return account ? decodeJuror(account.data) : null
}

export async function readVote(
  dispute: PublicKey,
  wallet: PublicKey,
): Promise<VoteCommitAccount | null> {
  const account = await connection.getAccountInfo(voteAddress(PROGRAM_ID, dispute, wallet))
  return account ? decodeVoteCommit(account.data) : null
}

/** A transaction ready for the wallet: fee payer and a fresh blockhash. */
export async function prepare(transaction: Transaction, feePayer: PublicKey) {
  const latest = await connection.getLatestBlockhash('confirmed')
  transaction.feePayer = feePayer
  transaction.recentBlockhash = latest.blockhash
  return latest
}

/**
 * Waits for the signature by polling its status rather than through
 * `confirmTransaction`: under load the latter can give up on a transaction
 * that did land. The blockhash bounds the wait — past its last valid height
 * the transaction can no longer land, and saying so is the honest answer.
 */
export async function waitFor(
  signature: string,
  lastValidBlockHeight: number,
): Promise<{ ok: true } | { ok: false; error: string }> {
  for (;;) {
    const { value } = await connection.getSignatureStatuses([signature])
    const status = value[0]
    if (status?.err) return { ok: false, error: JSON.stringify(status.err) }
    if (status?.confirmationStatus === 'confirmed' || status?.confirmationStatus === 'finalized') {
      return { ok: true }
    }
    const height = await connection.getBlockHeight('confirmed')
    if (height > lastValidBlockHeight) {
      // One last look: it may have landed in the very block that ran the
      // blockhash out.
      const last = (await connection.getSignatureStatuses([signature])).value[0]
      if (last && !last.err && last.confirmationStatus !== 'processed') return { ok: true }
      return { ok: false, error: 'The transaction expired before it landed' }
    }
    await new Promise((resolve) => setTimeout(resolve, 1_000))
  }
}
