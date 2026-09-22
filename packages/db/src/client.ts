import { drizzle } from 'drizzle-orm/postgres-js'
import postgres from 'postgres'
import * as schema from './schema.js'

/**
 * Підключення до Supabase через транзакційний пул (порт 6543), а не напряму.
 *
 * `prepare: false` — не налаштування смаку, а умова роботи: pgbouncer у
 * транзакційному режимі віддає з'єднання іншому клієнтові між запитами, і
 * підготовлений вираз, названий на одному з'єднанні, на наступному запиті вже
 * не існує. Виглядає це як `prepared statement "s1" does not exist` посеред
 * робочого коду, що жодного разу не падав локально.
 *
 * `max: 4` — стеля на процес. Free tier ділить пул між усіма, хто підключений,
 * включно з міграціями й дашбордом; API і watcher живуть в одному процесі й
 * одному пулі (`PLAN.md` → «Free tier capacity»), тож брати більше нема кому.
 */
export const clientOptions = { prepare: false, max: 4 } as const

export type Db = ReturnType<typeof createDb>

/**
 * Пул створюється **один раз на процес**. Другий виклик — другий пул, тобто
 * вдвічі більше з'єднань до спільного pgbouncer, ніж показує `max`.
 */
export function createDb(url: string) {
  return drizzle(postgres(url, clientOptions), { schema })
}
