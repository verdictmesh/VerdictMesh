import { readFileSync } from 'node:fs'
import { parseArgs } from 'node:util'
import {
  createAssociatedTokenAccountIdempotentInstruction,
  createTransferCheckedInstruction,
  getAccount,
  getAssociatedTokenAddressSync,
  getMint,
  TokenAccountNotFoundError,
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
import {
  fromBaseUnits,
  parseJurorList,
  shortfall,
  toBaseUnits,
  totalRequired,
} from './funding.js'

const envSchema = z.object({
  SOLANA_RPC_URL: z.url(),
  SETTLEMENT_MINT: z.string().min(32).max(44),
  TREASURY_KEYPAIR: z.string().min(1),
})

const { values } = parseArgs({
  options: {
    jurors: { type: 'string' },
    amount: { type: 'string', default: '100' },
    'dry-run': { type: 'boolean', default: false },
  },
})

if (values.jurors === undefined) {
  throw new Error('Usage: pnpm fund-jurors --jurors <file> [--amount 100] [--dry-run]')
}

const env = envSchema.parse(process.env)
const connection = new Connection(env.SOLANA_RPC_URL, 'confirmed')
const treasury = Keypair.fromSecretKey(bs58.decode(env.TREASURY_KEYPAIR))
const mintAddress = new PublicKey(env.SETTLEMENT_MINT)
const jurors = parseJurorList(readFileSync(values.jurors, 'utf8'))

// Decimals беремо з мінта, а не з константи: розходження між ними означає
// переказ, помилковий у 10^n разів, і виявиться воно на балансі.
const mint = await getMint(connection, mintAddress)
const target = toBaseUnits(values.amount, mint.decimals)

const treasuryAta = getAssociatedTokenAddressSync(mintAddress, treasury.publicKey)

async function currentBalance(owner: PublicKey): Promise<bigint> {
  const ata = getAssociatedTokenAddressSync(mintAddress, owner)
  try {
    return (await getAccount(connection, ata)).amount
  } catch (error) {
    if (error instanceof TokenAccountNotFoundError) return 0n
    throw error
  }
}

const plan = await Promise.all(
  jurors.map(async (juror) => ({
    juror,
    needed: shortfall(await currentBalance(juror), target),
  })),
)

const required = totalRequired(plan.map((entry) => entry.needed))
const available = await currentBalance(treasury.publicKey)

const human = (value: bigint) => fromBaseUnits(value, mint.decimals)
const pending = plan.filter((entry) => entry.needed > 0n)

console.log(`mint ${mintAddress.toBase58()} · decimals ${mint.decimals}`)
console.log(`jurors ${plan.length} · target ${values.amount} each · ${pending.length} need topping up`)
console.log(`treasury ${treasury.publicKey.toBase58()} holds ${human(available)}, needs ${human(required)}`)

// Суху прогонку не зупиняємо на нестачі: вона й існує, щоб дізнатися, скільки
// нести до крану. Падати має справжній запуск.
if (values['dry-run']) {
  for (const entry of pending) {
    console.log(`would send ${human(entry.needed)} to ${entry.juror.toBase58()}`)
  }
  if (available < required) {
    console.log(
      `top up the treasury by ${human(required - available)}: ` +
        `pnpm mint-to-treasury --amount ${human(required - available)}`,
    )
  }
  process.exit(0)
}

if (pending.length === 0) {
  console.log('Every juror is already funded — nothing to do.')
  process.exit(0)
}

// Впасти тут із точним числом дешевше, ніж роздати половині присяжних і стати
// посеред прогону. Казначейство наповнюється однією командою — воно ж і є mint
// authority розрахункового активу.
if (available < required) {
  throw new Error(
    `Treasury is short by ${human(required - available)}. ` +
      `Run: pnpm mint-to-treasury --amount ${human(required - available)}`,
  )
}

for (const entry of pending) {
  const ata = getAssociatedTokenAddressSync(mintAddress, entry.juror)
  const transaction = new Transaction().add(
    createAssociatedTokenAccountIdempotentInstruction(
      treasury.publicKey,
      ata,
      entry.juror,
      mintAddress,
    ),
    createTransferCheckedInstruction(
      treasuryAta,
      mintAddress,
      ata,
      treasury.publicKey,
      entry.needed,
      mint.decimals,
    ),
  )

  const signature = await sendAndConfirmTransaction(connection, transaction, [treasury])
  console.log(`sent ${human(entry.needed)} to ${entry.juror.toBase58()} — ${signature}`)
}
