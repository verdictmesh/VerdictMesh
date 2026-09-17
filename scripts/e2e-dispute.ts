/**
 * Наскрізний прогін на devnet — `SC-001`, `SC-002`, `SC-005`.
 *
 * Скрипт проходить повний цикл «спір → голосування → виплата» стільки разів,
 * скільки просить `--disputes`, і міряє три речі: скільки триває цикл, у скільки
 * обходяться його комісії всім учасникам разом і скільки разів довелося втрутитись
 * руками. Рішення «пройшов / не пройшов» рахує `./e2e.ts` — воно під тестами,
 * бо числа з devnet приходять повільно й один раз.
 *
 * **SDK ще немає** (T035), тож транзакції складаються тут вручну. Одна з них
 * несе два підписи: спір відкриває PDA ескроу через CPI, а депозит за розгляд
 * платить ініціатор власним ключем (`FR-026`) — і без його підпису VerdictMesh
 * такий спір не прийме.
 *
 * **Усі ключі прогону виводяться з ключа казначейства.** Відбір панелі отримує
 * реєстр цілком (`FR-006`), тож випадкові присяжні, згенеровані в пам'яті,
 * лишались би в ньому назавжди після кожного падіння — застейкані, придатні до
 * відбору і без приватного ключа. Детерміновані ключі роблять повторний запуск
 * тим самим запуском: реєстр не росте, а вже застейкані присяжні впізнаються.
 */

import { readFileSync } from 'node:fs'
import { parseArgs } from 'node:util'
import { AnchorProvider, BN, Program, Wallet } from '@coral-xyz/anchor'
import { keccak_256 } from '@noble/hashes/sha3'
import {
  createAssociatedTokenAccountIdempotentInstruction,
  getAssociatedTokenAddressSync,
  getMint,
  createMintToInstruction,
  TOKEN_PROGRAM_ID,
} from '@solana/spl-token'
import {
  Connection,
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  SystemProgram,
  SYSVAR_SLOT_HASHES_PUBKEY,
  Transaction,
  sendAndConfirmTransaction,
} from '@solana/web3.js'
import bs58 from 'bs58'
import { z } from 'zod'
import { cycleFloorSeconds, expectedOutcome, measure, type Ballot, type Run } from './e2e.js'
import type { ReferenceEscrow } from './idl/reference_escrow.js'
import type { VerdictMesh } from './idl/verdict_mesh.js'

const envSchema = z.object({
  SOLANA_RPC_URL: z.url(),
  SETTLEMENT_MINT: z.string().min(32).max(44),
  TREASURY_KEYPAIR: z.string().min(1),
})

const { values } = parseArgs({
  options: {
    disputes: { type: 'string', default: '20' },
    // Публічний devnet-RPC відповідає 429 уже на трьох паралельних циклах:
    // кожен із них це десяток транзакцій і стільки ж підтверджень. Ширшу хвилю
    // має сенс просити лише у власного вузла.
    batch: { type: 'string', default: '3' },
    'sol-price': { type: 'string', default: '200' },
  },
})

const env = envSchema.parse(process.env)
const disputeCount = Number(values.disputes)
const batchSize = Number(values.batch)
const solPriceUsd = Number(values['sol-price'])

const connection = new Connection(env.SOLANA_RPC_URL, 'confirmed')
const treasury = Keypair.fromSecretKey(bs58.decode(env.TREASURY_KEYPAIR))
const settlementMint = new PublicKey(env.SETTLEMENT_MINT)

const provider = new AnchorProvider(connection, new Wallet(treasury), {
  commitment: 'confirmed',
})

/**
 * IDL береться зі свіжої збірки, а тип — із копії в `./idl` (див. `sync-idl.ts`).
 * Розходження між ними стає червоним типечеком, а не сюрпризом на devnet.
 */
const readIdl = <T>(name: string): T =>
  JSON.parse(readFileSync(new URL(`../target/idl/${name}.json`, import.meta.url), 'utf8')) as T

const mesh = new Program<VerdictMesh>(readIdl<VerdictMesh>('verdict_mesh'), provider)
const escrowProgram = new Program<ReferenceEscrow>(
  readIdl<ReferenceEscrow>('reference_escrow'),
  provider,
)

// ── демо-конфігурація з docs/PLAN.md ────────────────────────────────────────

const DECIMALS = 6
const unit = (amount: number) => new BN(amount * 10 ** DECIMALS)

const POLICY = {
  panelSize: 3,
  extendedPanelSize: 5,
  quorum: 2,
  extendedQuorum: 3,
  // Поріг придатності, а не «скільки внести». 120 — вище за 100, зі стейком
  // якого в devnet-реєстрі лишились присяжні аварійного прогону (ключів до них
  // немає), і нижче за те, що зараз мають присяжні цього прогону. Придатність
  // міряється політикою спору (`FR-011a`), тож поріг — штатний спосіб не саджати
  // чужих. Жоден із трьох критеріїв від розміру стейку не залежить.
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
 * Панель голосує одностайно, і це вимушено.
 *
 * Розділена панель на вимірі з двадцяти спорів неможлива: `stake` створює запис
 * присяжного через `init`, доливати стейк нічим, а програний голос коштує 10%
 * (`FR-011`). Присяжний, що втратив частку, падає нижче порога придатності —
 * і більше не потрапляє в жодну панель, тобто не має як відіграти втрачене.
 * Реєстр із трьох на панель із трьох після першого ж розділеного голосування
 * не збирає панелі взагалі: `RegistryTooSmall`.
 *
 * Це властивість конструкції, а не збій прогону, і вона записана як знахідка
 * T025. Слешинг при цьому не лишається неперевіреним: його доводять 26 тестів
 * `settle_stakes.rs` і окремий прогін на одному спорі, де панель ділиться 2:1.
 */
const BALLOTS: Ballot[] = ['claimant', 'claimant', 'claimant']

/** Той самий вибір у вигляді, якого чекає Anchor від перелічуваного типу. */
const BALLOT_ARG = {
  claimant: { claimant: {} },
  respondent: { respondent: {} },
} as const

/**
 * Ключі прогону: та сама казна дає ту саму трійцю присяжних і ту саму
 * авторитетність інтегратора. Секрет казначейства входить у seed, тож ключі не
 * виводяться нізвідки, крім машини, яка вже й так ним розпоряджається.
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

const configPda = () => pda([Buffer.from('config')], mesh.programId)
const registryPda = () => pda([Buffer.from('registry')], mesh.programId)
const stakeVaultPda = () => pda([Buffer.from('stake_vault')], mesh.programId)
const integratorPda = (authority: PublicKey) =>
  pda([Buffer.from('integrator'), authority.toBuffer()], mesh.programId)
const disputePda = (integrator: PublicKey, id: BN) =>
  pda([Buffer.from('dispute'), integrator.toBuffer(), u64(id)], mesh.programId)
const disputeVaultPda = (dispute: PublicKey) =>
  pda([Buffer.from('dispute_vault'), dispute.toBuffer()], mesh.programId)
const votePda = (dispute: PublicKey, juror: PublicKey) =>
  pda([Buffer.from('vote'), dispute.toBuffer(), juror.toBuffer()], mesh.programId)
const jurorPda = (wallet: PublicKey) =>
  pda([Buffer.from('juror'), wallet.toBuffer()], mesh.programId)
const jurorIndexPda = (index: number) =>
  pda([Buffer.from('juror_idx'), new BN(index).toArrayLike(Buffer, 'le', 4)], mesh.programId)
const escrowPda = (buyer: PublicKey, dealId: BN) =>
  pda([Buffer.from('escrow'), buyer.toBuffer(), u64(dealId)], escrowProgram.programId)
const escrowVaultPda = (escrow: PublicKey) =>
  pda([Buffer.from('escrow_vault'), escrow.toBuffer()], escrowProgram.programId)
const bondVaultPda = (escrow: PublicKey) =>
  pda([Buffer.from('bond_vault'), escrow.toBuffer()], escrowProgram.programId)

/**
 * Відбиток голосу. Формула — з `programs/verdict-mesh/src/vote.rs`, і другого
 * джерела правди про неї немає: розбіжність тут дає голос, який неможливо
 * розкрити, і присяжного, слешеного за мовчання, якого він не обирав.
 */
const DOMAIN = Buffer.from('verdict_mesh/vote/v1')
const BALLOT_TAG: Record<Ballot, number> = { claimant: 1, respondent: 2 }

function commitmentOf(
  dispute: PublicKey,
  juror: PublicKey,
  choice: Ballot,
  salt: Uint8Array,
): Buffer {
  return Buffer.from(
    keccak_256(
      Buffer.concat([
        DOMAIN,
        dispute.toBuffer(),
        juror.toBuffer(),
        Buffer.from([BALLOT_TAG[choice]]),
        Buffer.from(salt),
      ]),
    ),
  )
}

// ── дрібні помічники ────────────────────────────────────────────────────────

/**
 * Вердикт зі стану спору. Anchor віддає перелічуваний тип об'єктом з єдиним
 * ключем; `null` тут неможливий — його не буває після вдалого `tally`, — але
 * мовчазне припущення про це зробило б помилку прогону схожою на помилку
 * протоколу.
 */
function outcomeOf(verdict: { claimant?: object; respondent?: object; statusQuo?: object } | null) {
  if (verdict === null) throw new Error('The dispute was tallied but carries no verdict')
  if (verdict.claimant !== undefined) return 'claimant' as const
  if (verdict.respondent !== undefined) return 'respondent' as const
  return 'statusQuo' as const
}

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, Math.max(0, ms)))

/**
 * Чекає, поки закриється вікно, за годинником **ланцюга**, а не за локальним.
 * Дедлайни записані в спорі в часі кластера, і на devnet він відходить від
 * настінного на секунди — досить, щоб інструкція прийшла на такт раніше.
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
 * Комісії — з ланцюга, а не з припущення про 5000 лампортів на підпис.
 *
 * Читаються **після** циклу, а не за кожною транзакцією одразу: `getTransaction`
 * — найдорожчий метод із тих, які тут потрібні, і публічний devnet-RPC відповідає
 * на нього 429 рівно тоді, коли цикл поспішає встигнути у вікно. Ціна виміру не
 * має входити у вимір.
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
        // 429 і мережеві збої — не результат, а затримка: транзакція вже
        // підтверджена, її комісія нікуди не подінеться.
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
 * Наповнення розрахунковим активом. Казначейство — це і mint authority, тож
 * учасники прогону отримують кошти емісією, а не переказом із запасу, якого
 * може не вистачити посеред прогону.
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

// ── підготовка протоколу ────────────────────────────────────────────────────

/**
 * `initialize` і `register_integrator` виконуються один раз на кластер, тож
 * прогін перевіряє наявність замість того, щоб покладатись на «має бути». Свій
 * `Integrator` він реєструє під власною виведеною авторитетністю — політика
 * прогону не має ані залежати від чужої, ані переписувати її.
 */
async function ensureProtocol(): Promise<PublicKey> {
  const config = configPda()

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

  // Друга авторитетність, а не перша: перший `Integrator` зареєстрований з
  // порогом 150, якого присяжні цього прогону вже не досягають — слешинг зняв
  // із одного з них 10%, а долити стейк нічим. Політика незмінна після
  // реєстрації (`FR-003`), тож потрібен новий інтегратор, а не правка старого.
  const authority = derivedKey('integrator', 1)
  const integrator = integratorPda(authority.publicKey)

  if ((await connection.getAccountInfo(integrator)) === null) {
    await fundWithSol([authority.publicKey], 0.02)
    await mesh.methods
      .registerIntegrator(POLICY)
      .accountsPartial({
        authority: authority.publicKey,
        integrator,
        config,
        escrowProgram: escrowProgram.programId,
        systemProgram: SystemProgram.programId,
      })
      .signers([authority])
      .rpc()
    console.log(`registered the integrator ${integrator.toBase58()}`)
  }

  // Скарбниця отримує частку протоколу в оплаті розгляду — токен-акаунт має
  // існувати до першого розрахунку, інакше падає вся фіналізація.
  await mintTo(treasury.publicKey, new BN(0))

  return integrator
}

interface Juror {
  keypair: Keypair
  slot: number
}

/**
 * Скільки SOL має тримати присяжний на весь прогін. Оренда акаунта голосу —
 * ~0.0017 SOL на розгляд і **не повертається**, тож сума росте разом із кількістю
 * спорів. Заміряно: прогін на 20 спорах із фінансуванням «на око» здихав на
 * шістнадцятому, і виглядало це як помилка протоколу, а не як порожній гаманець.
 */
const jurorSolNeeded = () => 0.01 + disputeCount * 0.0025

/**
 * Присяжні прогону. Уже застейканий впізнається за власним акаунтом і не
 * стейкається вдруге: `stake` створює його через `init`, тож повторний виклик
 * однаково відхилився б, а прогін має бути повторюваним.
 */
async function ensureJurors(): Promise<Juror[]> {
  const jurors: Juror[] = []

  for (let index = 0; index < POLICY.panelSize; index += 1) {
    const keypair = derivedKey('juror', index)
    const record = jurorPda(keypair.publicKey)
    const existing = await mesh.account.juror.fetchNullable(record)

    if (existing !== null) {
      // Долити SOL доводиться щоразу: оренду акаунта голосу присяжний платить
      // сам і назад не отримує — `VoteCommit` не закривається ніде (борг T020).
      // Тобто витрата присяжного лінійна від кількості розглядів, і прогін на
      // двадцять спорів коштує йому вчетверо більше за прогін на п'ять.
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

    const registry = await mesh.account.jurorRegistry.fetch(registryPda())
    const slot = registry.jurorCount

    await mesh.methods
      .stake(POLICY.jurorStake)
      .accountsPartial({
        juror: keypair.publicKey,
        config: configPda(),
        settlementMint,
        registry: registryPda(),
        jurorAccount: record,
        jurorIndex: jurorIndexPda(slot),
        jurorTokens,
        stakeVault: stakeVaultPda(),
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

/** Увесь реєстр парами (`JurorIndex`, `Juror`) — саме цього чекає відбір. */
async function registryAccounts() {
  const registry = await mesh.account.jurorRegistry.fetch(registryPda())
  const slots = Array.from({ length: registry.jurorCount }, (_, slot) => slot)

  return Promise.all(
    slots.map(async (slot) => {
      const index = jurorIndexPda(slot)
      const entry = await mesh.account.jurorIndex.fetch(index)
      return [
        { pubkey: index, isSigner: false, isWritable: false },
        { pubkey: jurorPda(entry.wallet), isSigner: false, isWritable: true },
      ]
    }),
  ).then((pairs) => pairs.flat())
}

// ── один цикл ───────────────────────────────────────────────────────────────

async function runCycle(
  index: number,
  integrator: PublicKey,
  jurors: Juror[],
  registry: Awaited<ReturnType<typeof registryAccounts>>,
): Promise<Run> {
  const interventions: string[] = []
  const signatures: string[] = []

  const buyer = Keypair.generate()
  const seller = Keypair.generate()
  const dealId = new BN(index)

  const escrow = escrowPda(buyer.publicKey, dealId)
  const total = MILESTONES.reduce((sum, amount) => sum.add(amount), new BN(0))
  const bonds = POLICY.deposit.muln(MILESTONES.length)

  // Замовник платить оренду за угоду, її касу і касу застав; виконавець — за
  // акаунт спору і його сховище. Разом ~0.007 у кожного; решта — запас на
  // комісії. Щедріше фінансування просто замикає devnet-SOL у гаманцях, які
  // після прогону вже нікому не потрібні.
  await fundWithSol([buyer.publicKey, seller.publicKey], 0.012)
  const buyerTokens = await mintTo(buyer.publicKey, total.add(bonds))
  // Виконавець вносить свою заставу і депозит за розгляд: спір відкриває він.
  const sellerTokens = await mintTo(seller.publicKey, bonds.add(POLICY.deposit))

  await escrowProgram.methods
    .createEscrow(dealId, MILESTONES)
    .accountsPartial({
      buyer: buyer.publicKey,
      seller: seller.publicKey,
      mint: settlementMint,
      integrator,
      config: configPda(),
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

  // Цикл починається тут: `SC-001` міряє «спір → голосування → виплата», а не
  // укладання угоди, яке відбувається з розглядом і без нього однаково.
  const started = Date.now()

  const integratorAccount = await mesh.account.integrator.fetch(integrator)
  const dispute = disputePda(integrator, integratorAccount.disputeCount)

  // Відкриття спору і відбір панелі — одна транзакція. Панель визначена слотом,
  // записаним при відкритті (`FR-006`), тож розносити їх у часі немає причин, а
  // тримати спір без панелі — є ризик.
  const opening = await escrowProgram.methods
    .disputeMilestone(DISPUTED_MILESTONE)
    .accountsPartial({
      claimant: seller.publicKey,
      escrow,
      integrator,
      config: configPda(),
      settlementMint,
      dispute,
      claimantTokens: sellerTokens,
      disputeVault: disputeVaultPda(dispute),
      verdictMeshProgram: mesh.programId,
      tokenProgram: TOKEN_PROGRAM_ID,
      systemProgram: SystemProgram.programId,
    })
    .postInstructions([
      await mesh.methods
        .selectPanel()
        .accountsPartial({
          dispute,
          registry: registryPda(),
          slotHashes: SYSVAR_SLOT_HASHES_PUBKEY,
        })
        .remainingAccounts(registry)
        .instruction(),
    ])
    .signers([seller])
    .rpc()
  signatures.push(opening)

  const opened = await mesh.account.dispute.fetch(dispute)
  const panel = opened.panel.map((wallet) => wallet.toBase58())

  // Голоси роздаються за місцями в панелі, а не за порядком стейкання: відбір
  // детермінований, але не тотожний реєстру.
  const seated = panel.map((wallet, seat) => {
    const juror = jurors.find((candidate) => candidate.keypair.publicKey.toBase58() === wallet)
    // Реєстр спільний на весь протокол, і в ньому можуть сидіти присяжні, яких
    // цей прогін не заводив. Політика прогону їх не пускає порогом стейку — але
    // якщо один усе ж сів, це не збій скрипта, а розгляд, який без рук не
    // закінчиться, і `SC-005` мусить це побачити.
    if (juror === undefined) {
      throw new Error(`Panel seat ${seat} went to ${wallet}, a juror this run cannot vote for`)
    }
    return {
      juror,
      choice: BALLOTS[seat] ?? 'claimant',
      salt: derivedKey(`salt/${dispute.toBase58()}`, seat).secretKey.slice(0, 32),
    }
  })

  for (const member of seated) {
    const signature = await mesh.methods
      .commitVote([
        ...commitmentOf(dispute, member.juror.keypair.publicKey, member.choice, member.salt),
      ])
      .accountsPartial({
        juror: member.juror.keypair.publicKey,
        dispute,
        vote: votePda(dispute, member.juror.keypair.publicKey),
        systemProgram: SystemProgram.programId,
      })
      .signers([member.juror.keypair])
      .rpc()
    signatures.push(signature)
  }

  await waitForDeadline(opened.commitDeadline.toNumber())

  for (const member of seated) {
    const signature = await mesh.methods
      .revealVote(BALLOT_ARG[member.choice], [...member.salt])
      .accountsPartial({
        juror: member.juror.keypair.publicKey,
        dispute,
        vote: votePda(dispute, member.juror.keypair.publicKey),
      })
      .signers([member.juror.keypair])
      .rpc()
    signatures.push(signature)
  }

  await waitForDeadline(opened.revealDeadline.toNumber())
  signatures.push(await mesh.methods.tally().accountsPartial({ dispute }).rpc())

  const tallied = await mesh.account.dispute.fetch(dispute)
  const verdict = outcomeOf(tallied.verdict)

  await waitForDeadline(tallied.appealDeadline.toNumber())

  const settlement = await escrowProgram.methods
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
    .rpc()
  signatures.push(settlement)

  // Виплата відбулась — цикл `SC-001` закінчився. Розрахунок стейків панелі йде
  // після і на нього не чекають: це внутрішня справа протоколу, і тримати чужі
  // кошти замкненими доти було б рівно тим, чого `settle_milestone` не робить.
  const elapsedMs = Date.now() - started

  const settleStakes = await mesh.methods
    .settleStakes()
    .accountsPartial({
      dispute,
      crank: treasury.publicKey,
      config: configPda(),
      settlementMint,
      disputeVault: disputeVaultPda(dispute),
      stakeVault: stakeVaultPda(),
      treasuryTokens: getAssociatedTokenAddressSync(settlementMint, treasury.publicKey),
      tokenProgram: TOKEN_PROGRAM_ID,
    })
    .remainingAccounts(
      seated.flatMap((member) => [
        { pubkey: jurorPda(member.juror.keypair.publicKey), isSigner: false, isWritable: true },
        {
          pubkey: votePda(dispute, member.juror.keypair.publicKey),
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

// ── прогін ──────────────────────────────────────────────────────────────────

const mint = await getMint(connection, settlementMint)
if (mint.decimals !== DECIMALS) {
  throw new Error(`Settlement mint has ${mint.decimals} decimals, the demo policy assumes ${DECIMALS}`)
}

console.log(`arbitration ${mesh.programId.toBase58()} · escrow ${escrowProgram.programId.toBase58()}`)
console.log(
  `${disputeCount} disputes in waves of ${batchSize} · floor ${cycleFloorSeconds({ ...POLICY, commitWindow: 60, revealWindow: 60, appealWindow: 90 })}s per cycle`,
)

const integrator = await ensureProtocol()
const jurors = await ensureJurors()
const registry = await registryAccounts()

/**
 * Відмови RPC, що прилетіли повз ланцюг промісів. `@solana/web3.js` віддає
 * частину помилок транспорту через колбек, який ніхто не чекає, — і Node вбиває
 * процес посеред виміру. Мовчки проковтнути їх не можна: це рівно те, що `SC-005`
 * називає «знадобились руки», тож вони приписуються хвилі, під час якої сталися.
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

  // Цикли хвилі розводяться в часі: одночасний старт кладе на RPC десяток
  // транзакцій в одну мить, і відповідь на це — 429 та «blockhash not found»,
  // тобто провал виміру з причини, що до протоколу не має стосунку.
  const results = await Promise.allSettled(
    wave.map(async (index, offset) => {
      await sleep(offset * 3000)
      return runCycle(index, integrator, jurors, registry)
    }),
  )

  // Те, що зірвалось повз проміси, належить цій хвилі — приписуємо першому ж її
  // циклу, бо точніше не встановити, а втратити не можна.
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

    // Цикл, що впав, — не відсутній результат, а найважливіший із них: `SC-005`
    // міряє саме частку розглядів, доведених без рук.
    console.error(`dispute ${wave[offset]} needed a hand: ${result.reason}`)
    runs.push({
      dispute: `deal ${wave[offset]}`,
      elapsedMs: Number.POSITIVE_INFINITY,
      feeLamports: 0n,
      outcome: 'statusQuo',
      expected: 'statusQuo',
      interventions: [
        `deal ${wave[offset]}: ${result.reason}`,
        ...(offset === 0 ? strayHere : []),
      ],
    })
  })
}

const report = measure(runs, { cycleSeconds: 300, cycleUsd: 0.1, solPriceUsd })

console.log('')
console.log(`SC-001 full cycle      ${report.sc001.passed ? 'PASS' : 'FAIL'} · slowest ${(report.sc001.slowestMs / 1000).toFixed(1)}s of ${report.sc001.budgetMs / 1000}s`)
console.log(`SC-002 fees per cycle  ${report.sc002.passed ? 'PASS' : 'FAIL'} · worst $${report.sc002.worstUsd.toFixed(4)} of $${report.sc002.budgetUsd} at $${solPriceUsd}/SOL`)
console.log(`SC-005 no hands needed ${report.sc005.passed ? 'PASS' : 'FAIL'} · ${report.sc005.automatic} of ${report.sc005.total} automatic`)
for (const line of report.sc005.needed) console.log(`  ${line}`)

process.exit(report.sc001.passed && report.sc002.passed && report.sc005.passed ? 0 : 1)
