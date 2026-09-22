import type { FactFindingReport } from '@verdictmesh/shared'
import { sql } from 'drizzle-orm'
import type { AnyPgColumn } from 'drizzle-orm/pg-core'
import {
  bigint,
  boolean,
  char,
  check,
  index,
  integer,
  jsonb,
  numeric,
  pgEnum,
  pgPolicy,
  pgTable,
  primaryKey,
  text,
  timestamp,
} from 'drizzle-orm/pg-core'
import { anonRole, authenticatedRole } from 'drizzle-orm/supabase'

/**
 * Дзеркало ончейн-стану плюс те, чого в ланцюгу немає: тіла звітів і зібрані
 * докази. Джерело правди — ланцюг; втрата бази коштує швидкості читання, а не
 * спорів, голосів чи коштів (`PLAN.md` → «Модель даних»).
 *
 * Чотири наскрізні рішення:
 *
 * **Суми — `numeric(20, 0)`, ніколи `bigint`.** `int8` знаковий, тобто
 * закінчується на 2^63−1, а на вісі `amount` — це `u64`. Половина діапазону
 * лягла б помилкою Postgres у найгіршу мить: на спорі з великою сумою.
 * `numeric(20, 0)` тримає весь `u64` точно, а `mode: 'bigint'` не дає числу
 * стати `number` і загубити молодші розряди по дорозі.
 *
 * **Слоти й дедлайни — `bigint({ mode: 'number' })`.** Це `u64` і `i64` на
 * вісі, але слот не дійде до 2^53 за життя мережі, а `i64` тут — unix-час у
 * секундах. `DisputeView` віддає їх як `number`, і зайве перетворення на межі
 * було б місцем, де числа розходяться.
 *
 * **Хеші — `char(64)` у нижньому регістрі, ключі й підписи — `text` у base58.**
 * Ті самі рядки, що в API і в `packages/shared`, тож перетворень між шарами
 * немає. Регістр закріплений `check`, бо `FR-017b` звіряє відбиток звіту
 * порівнянням рядків: hex у верхньому регістрі не збігся б із тим самим хешем.
 *
 * **Таблиця спорів тримає рівно те, що watcher (T027) бачить у самому акаунті
 * `Dispute`.** Подій для неї не досить: `DisputeOpened` не несе дедлайнів, бо
 * вони виводяться з політики, — тож watcher однаково читає акаунт, і межа
 * «скільки кешувати» проходить по межі акаунта, а не по складу подій. Чого тут
 * немає свідомо: `policy` (знімок цілком, потрібен лише програмі),
 * `entropy_slot` (перевірка відбору, `FR-006a`, читається з ланцюга на вимогу) і
 * `dispute_id` (адреса PDA і є ідентичністю).
 */

/** base58 — той самий рядок, що в `DisputeView` і в гаманці. */
const base58 = (name: string) => text(name)
/** Нижній регістр hex, 32 байти. Довжину тримає тип, регістр — `check`. */
const hex64 = (name: string) => char(name, { length: 64 })
/** Базові одиниці розрахункового активу. `u64` цілком — див. рішення вище. */
const u64 = (name: string) => numeric(name, { precision: 20, scale: 0, mode: 'bigint' })
/** Unix-час у секундах або номер слота. */
const chainTime = (name: string) => bigint(name, { mode: 'number' })

const lowerHex = (name: string, column: AnyPgColumn) =>
  check(name, sql`${column} ~ '^[0-9a-f]{64}$'`)

/**
 * Значення повторюють `packages/shared`: enum бази і enum контракту — одна
 * множина, і розходження ловить тест, а не перший невдалий `INSERT`.
 */
export const disputeStateEnum = pgEnum('dispute_state', [
  'OptimisticPending',
  'Committing',
  'Revealing',
  'Tallied',
  'Appealed',
  'Finalized',
])

export const verdictEnum = pgEnum('verdict', ['Claimant', 'Respondent', 'StatusQuo'])

/**
 * `FR-016` знає рівно два джерела факту: підпис транзакції і адреса акаунта.
 * Третього вигляду посилання звіт не вміє показати, тож і в доказах його немає.
 */
export const evidenceKindEnum = pgEnum('evidence_kind', ['transaction', 'account'])

/**
 * Обидві публічні ролі Supabase не бачать нічого. `RESTRICTIVE`, а не просто
 * увімкнений RLS без політик: restrictive-політики поєднуються через AND, тож
 * permissive-політика, дописана колись через дашборд, таблицю вже не відкриє.
 * Сервіс ходить службовою роллю, а її RLS не стосується.
 *
 * `PLAN.md` обіцяв публічне читання звітів і доказів. Тут його немає, і це
 * свідома зміна: єдиний спосіб прочитати звіт правильно — через `GET
 * /disputes/:pda/report`, який віддає його разом із `matchesOnchain` (`FR-017b`).
 * Другий шлях читання, що віддає тіло звіту **без** звірки з відбитком у стані
 * спору, — це рівно те місце, де підмінений звіт виглядає справжнім. Читача,
 * який ходив би анонімним ключем повз API, у системі немає: `apps/web`
 * розмовляє лише з `apps/api`. Якщо такий читач колись зʼявиться, політику
 * додають разом із перевіркою хеша на його боці, а не раніше.
 */
const denyAll = (table: string) =>
  pgPolicy(`${table}_deny_all`, {
    as: 'restrictive',
    for: 'all',
    to: [anonRole, authenticatedRole],
    using: sql`false`,
    withCheck: sql`false`,
  })

export const disputes = pgTable(
  'disputes',
  {
    /** Адреса PDA спору. Ідентичність спору — вона, а не лічильник. */
    pda: base58('pda').primaryKey(),
    integrator: base58('integrator').notNull(),
    /** PDA ескроу, який відкрив спір. За ним T028 збирає докази. */
    escrowRef: base58('escrow_ref').notNull(),
    claimant: base58('claimant').notNull(),
    respondent: base58('respondent').notNull(),
    amount: u64('amount').notNull(),
    state: disputeStateEnum('state').notNull(),
    /** Порожній до відбору панелі; після ескалації доростає (`FR-027`). */
    panel: base58('panel').array().notNull(),
    /** `null`, поки reporter не записав відбиток. На вісі це нулі (`FR-017`). */
    reportHash: hex64('report_hash'),
    /**
     * Відбитки позицій сторін (`FR-005`). Тексту позицій у базі немає навмисно:
     * ескроу виводить їх із ролей (`claims.rs`, T022), а T029 повторює ту саму
     * формулу офчейн. Дві копії формули розходяться мовчки — ці два хеші і є
     * єдиний спосіб помітити розходження, не читаючи акаунт на кожен звіт.
     */
    claimantClaimHash: hex64('claimant_claim_hash').notNull(),
    respondentClaimHash: hex64('respondent_claim_hash').notNull(),
    openedAt: chainTime('opened_at').notNull(),
    commitDeadline: chainTime('commit_deadline').notNull(),
    revealDeadline: chainTime('reveal_deadline').notNull(),
    /** Нуль, поки немає вердикту: вікно апеляції відлічується від нього. */
    appealDeadline: chainTime('appeal_deadline').notNull(),
    /**
     * Розкриті голоси. Ескалація їх не скидає — розширена панель продовжує той
     * самий розгляд (`tally.rs`), тож сума росте через обидва кола.
     */
    votesClaimant: integer('votes_claimant').notNull(),
    votesRespondent: integer('votes_respondent').notNull(),
    escalated: boolean('escalated').notNull(),
    verdict: verdictEnum('verdict'),
    /** Слот, на якому знято дзеркало: старіше не перезаписує новіше. */
    syncedSlot: chainTime('synced_slot').notNull(),
  },
  (table) => [
    // Перший екран панелі присяжного (`SC-010`): відкриті спори, найстаріші
    // першими. Без цього список будується сортуванням усієї таблиці.
    index('disputes_state_opened_idx').on(table.state, table.openedAt),
    index('disputes_integrator_idx').on(table.integrator),
    // `FR-019`: «спори, доступні цьому присяжному» — це пошук гаманця в
    // масиві панелі, і саме його GIN і обслуговує.
    index('disputes_panel_idx').using('gin', table.panel),
    check('disputes_amount_non_negative', sql`${table.amount} >= 0`),
    check('disputes_votes_claimant_non_negative', sql`${table.votesClaimant} >= 0`),
    check('disputes_votes_respondent_non_negative', sql`${table.votesRespondent} >= 0`),
    check('disputes_synced_slot_non_negative', sql`${table.syncedSlot} >= 0`),
    // Вікна складаються одне за одним: розкриття після подання.
    check('disputes_reveal_after_commit', sql`${table.revealDeadline} > ${table.commitDeadline}`),
    // Нуль тут означає «вердикту ще не було», а не дедлайн у 1970-му.
    check(
      'disputes_appeal_after_reveal',
      sql`${table.appealDeadline} = 0 or ${table.appealDeadline} >= ${table.revealDeadline}`,
    ),
    // Стан і вердикт — одне твердження, записане двічі, тож нехай база й тримає
    // їх разом. Рядок «Finalized без вердикту» або «Committing з вердиктом» не
    // існує на вісі й не має існувати в дзеркалі.
    check(
      'disputes_verdict_matches_state',
      sql`(${table.verdict} is null) = (${table.state} in ('OptimisticPending', 'Committing', 'Revealing'))`,
    ),
    lowerHex('disputes_report_hash_lower_hex', table.reportHash),
    lowerHex('disputes_claimant_claim_hash_lower_hex', table.claimantClaimHash),
    lowerHex('disputes_respondent_claim_hash_lower_hex', table.respondentClaimHash),
    denyAll('disputes'),
  ],
).enableRLS()

/**
 * Тіло fact-finding звіту. Версія, а не перезапис: `FR-018` дозволяє звіту
 * зʼявитись із запізненням або не зʼявитись зовсім, а `FR-017` робить відбиток
 * незмінним після запису. Друга спроба генерації, що затерла б першу, забрала б
 * саме той рядок, з яким збігається відбиток у стані спору.
 */
export const reports = pgTable(
  'reports',
  {
    disputePda: base58('dispute_pda')
      .notNull()
      .references(() => disputes.pda),
    version: integer('version').notNull(),
    content: jsonb('content').$type<FactFindingReport>().notNull(),
    /** sha256 канонічних байтів `content`. Звіряється з `disputes.report_hash`. */
    contentHash: hex64('content_hash').notNull(),
    /** Ідентифікатор моделі повністю: звіти різних версій моделі незрівнянні. */
    model: text('model').notNull(),
    createdAt: timestamp('created_at', { withTimezone: true }).notNull().defaultNow(),
  },
  (table) => [
    primaryKey({ columns: [table.disputePda, table.version] }),
    check('reports_version_positive', sql`${table.version} >= 1`),
    lowerHex('reports_content_hash_lower_hex', table.contentHash),
    denyAll('reports'),
  ],
).enableRLS()

/**
 * Зібрані ончейн-факти, з яких будується звіт (`FR-015`).
 *
 * `PLAN.md` малював тут дві колонки — `signature` і `account`. Вони злились в
 * одну: ключ таблиці мусить бути `NOT NULL`, а дві колонки, з яких рівно одна
 * заповнена, вимагали б `check`, який повторював би те, що вже сказав `kind`.
 * На межі API розгалуження однаково є — `evidenceRef` у `packages/shared`
 * розрізняє `sourceSignature` і `sourceAccount`.
 */
export const evidence = pgTable(
  'evidence',
  {
    disputePda: base58('dispute_pda')
      .notNull()
      .references(() => disputes.pda),
    kind: evidenceKindEnum('kind').notNull(),
    /** Підпис транзакції або адреса акаунта, за `kind`. Обидва — base58. */
    source: base58('source').notNull(),
    /** Слот транзакції або слот знімка акаунта. За ним будується хронологія. */
    slot: chainTime('slot').notNull(),
    payload: jsonb('payload').$type<Record<string, unknown>>().notNull(),
    fetchedAt: timestamp('fetched_at', { withTimezone: true }).notNull().defaultNow(),
  },
  (table) => [
    primaryKey({ columns: [table.disputePda, table.source] }),
    index('evidence_dispute_slot_idx').on(table.disputePda, table.slot),
    check('evidence_slot_non_negative', sql`${table.slot} >= 0`),
    denyAll('evidence'),
  ],
).enableRLS()
