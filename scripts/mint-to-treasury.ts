import { parseArgs } from 'node:util'
import {
  createAssociatedTokenAccountIdempotentInstruction,
  createMintToCheckedInstruction,
  getAssociatedTokenAddressSync,
  getMint,
} from '@solana/spl-token'
import {
  Connection,
  Keypair,
  PublicKey,
  sendAndConfirmTransaction,
  Transaction,
} from '@solana/web3.js'
import bs58 from 'bs58'
import { z } from 'zod'
import { fromBaseUnits, toBaseUnits } from './funding.js'

/**
 * Доемітує розрахунковий актив у казначейство.
 *
 * Це та половина рішення «власний mint замість крану Circle», без якої друга
 * половина безглузда: якщо казначейство спорожніло, наповнити його має бути
 * командою, а не походом на чужий сайт. `fund-jurors.ts` посилається саме сюди,
 * коли рахує нестачу.
 */
const envSchema = z.object({
  SOLANA_RPC_URL: z.url(),
  SETTLEMENT_MINT: z.string().min(32).max(44),
  TREASURY_KEYPAIR: z.string().min(1),
})

const { values } = parseArgs({
  options: {
    amount: { type: 'string' },
  },
})

if (values.amount === undefined) {
  throw new Error('Usage: pnpm mint-to-treasury --amount <whole tokens>')
}

const env = envSchema.parse(process.env)
const connection = new Connection(env.SOLANA_RPC_URL, 'confirmed')
const treasury = Keypair.fromSecretKey(bs58.decode(env.TREASURY_KEYPAIR))
const mintAddress = new PublicKey(env.SETTLEMENT_MINT)

// Decimals читаємо з мінта, а не з константи — розходження між ними означає
// емісію, помилкову у 10^n разів.
const mint = await getMint(connection, mintAddress)

if (mint.mintAuthority === null || !mint.mintAuthority.equals(treasury.publicKey)) {
  throw new Error(
    `Treasury ${treasury.publicKey.toBase58()} is not the mint authority of ${mintAddress.toBase58()} ` +
      `(authority: ${mint.mintAuthority?.toBase58() ?? 'none'}). Nothing to mint with.`,
  )
}

const amount = toBaseUnits(values.amount, mint.decimals)
const treasuryAta = getAssociatedTokenAddressSync(mintAddress, treasury.publicKey)

const transaction = new Transaction().add(
  createAssociatedTokenAccountIdempotentInstruction(
    treasury.publicKey,
    treasuryAta,
    treasury.publicKey,
    mintAddress,
  ),
  createMintToCheckedInstruction(
    mintAddress,
    treasuryAta,
    treasury.publicKey,
    amount,
    mint.decimals,
  ),
)

const signature = await sendAndConfirmTransaction(connection, transaction, [treasury])

console.log(`minted ${fromBaseUnits(amount, mint.decimals)} to ${treasuryAta.toBase58()}`)
console.log(`signature ${signature}`)
