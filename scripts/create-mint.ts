import { parseArgs } from 'node:util'
import {
  createAssociatedTokenAccountIdempotentInstruction,
  createInitializeMint2Instruction,
  createMintToCheckedInstruction,
  getAssociatedTokenAddressSync,
  getMinimumBalanceForRentExemptMint,
  MINT_SIZE,
  TOKEN_PROGRAM_ID,
} from '@solana/spl-token'
import {
  Connection,
  Keypair,
  sendAndConfirmTransaction,
  SystemProgram,
  Transaction,
} from '@solana/web3.js'
import bs58 from 'bs58'
import { z } from 'zod'
import { fromBaseUnits, toBaseUnits } from './funding.js'

/**
 * Створює розрахунковий актив демо (`FR-011a`) і наповнює ним казначейство.
 *
 * `SPEC.md` вимагає стабільний актив, спільний для всього протоколу, і прямо
 * допускає на devnet тестовий mint. Спершу брали публічний devnet-USDC від
 * Circle, але його кран закритий geo-блоком — а залежність, яка може відмовити
 * за годину до показу, гірша за власний mint із тими самими 6 знаками.
 *
 * **Freeze authority навмисно порожній.** Ключ, здатний заморозити токен-акаунт,
 * заморожує і стейки присяжних, і виплату переможцю — тобто це рівно та влада
 * над чужими коштами, якої за `FR-014` у системі бути не повинно. Mint authority
 * лишається у казначейства, бо без неї не наповнити присяжних; програма не
 * емітує нічого й цього ключа не бачить.
 */
const envSchema = z.object({
  SOLANA_RPC_URL: z.url(),
  TREASURY_KEYPAIR: z.string().min(1),
})

const DECIMALS = 6

const { values } = parseArgs({
  options: {
    supply: { type: 'string', default: '10000' },
  },
})

const env = envSchema.parse(process.env)
const connection = new Connection(env.SOLANA_RPC_URL, 'confirmed')
const treasury = Keypair.fromSecretKey(bs58.decode(env.TREASURY_KEYPAIR))
const mint = Keypair.generate()

const supply = toBaseUnits(values.supply, DECIMALS)
const treasuryAta = getAssociatedTokenAddressSync(mint.publicKey, treasury.publicKey)

const transaction = new Transaction().add(
  SystemProgram.createAccount({
    fromPubkey: treasury.publicKey,
    newAccountPubkey: mint.publicKey,
    space: MINT_SIZE,
    lamports: await getMinimumBalanceForRentExemptMint(connection),
    programId: TOKEN_PROGRAM_ID,
  }),
  createInitializeMint2Instruction(mint.publicKey, DECIMALS, treasury.publicKey, null),
  createAssociatedTokenAccountIdempotentInstruction(
    treasury.publicKey,
    treasuryAta,
    treasury.publicKey,
    mint.publicKey,
  ),
  // Checked-варіант: він звіряє decimals із самим мінтом. Помилка в порядку
  // величини тут не впала б, а просто надрукувала б не ту суму.
  createMintToCheckedInstruction(mint.publicKey, treasuryAta, treasury.publicKey, supply, DECIMALS),
)

const signature = await sendAndConfirmTransaction(connection, transaction, [treasury, mint])

console.log(`mint      ${mint.publicKey.toBase58()} · decimals ${DECIMALS}`)
console.log(`authority mint=${treasury.publicKey.toBase58()} freeze=none`)
console.log(`treasury  ${treasuryAta.toBase58()} holds ${fromBaseUnits(supply, DECIMALS)}`)
console.log(`signature ${signature}`)
console.log(`\nSet SETTLEMENT_MINT=${mint.publicKey.toBase58()}`)
