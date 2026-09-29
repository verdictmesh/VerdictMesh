import { utils } from '@coral-xyz/anchor'
import type { Connection } from '@solana/web3.js'
import { Keypair, PublicKey, Transaction, TransactionInstruction } from '@solana/web3.js'
import { verdictMeshIdl } from './idl/verdict-mesh.js'

/**
 * Writing the report fingerprint on chain — `attest_report`, signed by the
 * reporter key (`FR-017`). The instruction is the only thing that key can sign
 * in the program, and this file is the only place the service uses it.
 */

export interface Attester {
  /**
   * Records `hash` (lowercase hex) as the fingerprint of the dispute's report
   * and resolves with the transaction signature once the chain has it.
   */
  attest(disputePda: string, hash: string): Promise<string>
}

const idlInstruction = () => {
  const instruction = verdictMeshIdl.instructions.find(
    (candidate) => candidate.name === 'attest_report',
  )
  if (!instruction) throw new Error('Vendored IDL has no attest_report instruction')
  return instruction
}

/**
 * The reporter key from the environment — a base58 secret key, the form
 * wallets export. The error never repeats the value: it is the one secret in
 * the service that can sign on chain.
 */
export function reporterKeypair(secret: string): Keypair {
  try {
    return Keypair.fromSecretKey(utils.bytes.bs58.decode(secret.trim()))
  } catch {
    throw new Error('REPORTER_KEYPAIR is not a base58 secret key')
  }
}

const HEX_HASH = /^[0-9a-f]{64}$/

export const configPda = (programId: PublicKey): PublicKey =>
  PublicKey.findProgramAddressSync([Buffer.from('config')], programId)[0]

/**
 * The instruction itself, without the network. Account order and flags are
 * the IDL's rather than written out here, so a reordered `#[derive(Accounts)]`
 * shows up as a red test instead of a transaction the program rejects.
 */
export function attestInstruction(
  programId: PublicKey,
  reporter: PublicKey,
  disputePda: PublicKey,
  hash: string,
): TransactionInstruction {
  // An all-zero fingerprint is "no report" on chain and the program refuses
  // it; anything that is not 32 bytes of hex is a bug upstream, not a report.
  if (!HEX_HASH.test(hash) || /^0+$/.test(hash)) {
    throw new Error(`Not a report fingerprint: ${hash}`)
  }

  const instruction = idlInstruction()
  const addresses: Record<string, PublicKey> = {
    reporter,
    config: configPda(programId),
    dispute: disputePda,
  }

  const keys = instruction.accounts.map((account) => {
    const pubkey = addresses[account.name]
    if (!pubkey) throw new Error(`attest_report asks for an unknown account: ${account.name}`)
    return {
      pubkey,
      isSigner: 'signer' in account && account.signer === true,
      isWritable: 'writable' in account && account.writable === true,
    }
  })

  return new TransactionInstruction({
    programId,
    keys,
    data: Buffer.concat([Buffer.from(instruction.discriminator), Buffer.from(hash, 'hex')]),
  })
}

export interface SolanaAttesterOptions {
  /** Milliseconds between status polls. */
  pollMs?: number
  sleep?: (ms: number) => Promise<void>
}

/**
 * Over RPC, with the reporter paying its own fee — the key needs a little SOL
 * and nothing else.
 *
 * Confirmation is polled here instead of going through `confirmTransaction`:
 * web3.js 1.x starts an unawaited status request inside it, and a 429 on that
 * request is a rejection nobody owns, which takes the whole process down. The
 * poll also ends on the blockhash expiring rather than on a wall-clock guess.
 */
export function solanaAttester(
  connection: Connection,
  programId: PublicKey,
  reporter: Keypair,
  options: SolanaAttesterOptions = {},
): Attester {
  const pollMs = options.pollMs ?? 1_000
  const sleep = options.sleep ?? ((ms) => new Promise((resolve) => setTimeout(resolve, ms)))

  return {
    async attest(disputePda, hash) {
      const instruction = attestInstruction(
        programId,
        reporter.publicKey,
        new PublicKey(disputePda),
        hash,
      )
      const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash('confirmed')
      const transaction = new Transaction({
        feePayer: reporter.publicKey,
        blockhash,
        lastValidBlockHeight,
      }).add(instruction)
      transaction.sign(reporter)

      // Preflight stays on: a refusal by the program (window closed, already
      // attested) comes back here with its logs instead of costing a fee.
      const signature = await connection.sendRawTransaction(transaction.serialize(), {
        preflightCommitment: 'confirmed',
      })

      for (;;) {
        const { value } = await connection.getSignatureStatuses([signature])
        const status = value[0]
        if (status?.err) {
          throw new Error(`attest_report ${signature} failed: ${JSON.stringify(status.err)}`)
        }
        if (
          status?.confirmationStatus === 'confirmed' ||
          status?.confirmationStatus === 'finalized'
        ) {
          return signature
        }
        const height = await connection.getBlockHeight('confirmed')
        if (height > lastValidBlockHeight) {
          throw new Error(`attest_report ${signature} expired unconfirmed`)
        }
        await sleep(pollMs)
      }
    },
  }
}
