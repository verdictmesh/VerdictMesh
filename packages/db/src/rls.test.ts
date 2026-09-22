import { readdirSync, readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { getTableName, is } from 'drizzle-orm'
import { PgTable } from 'drizzle-orm/pg-core'
import { describe, expect, it } from 'vitest'
import * as schema from './schema.js'

/**
 * Перелік таблиць береться зі схеми, а не з константи: четверта таблиця,
 * додана без RLS, має розсипати гейт, а не поїхати на Supabase відкритою.
 *
 * Перевіряється саме SQL міграцій, а не наміри в схемі: на базу їде він.
 */

const migrations = join(dirname(fileURLToPath(import.meta.url)), '../migrations')

const sql = readdirSync(migrations)
  .filter((file) => file.endsWith('.sql'))
  .sort()
  .map((file) => readFileSync(join(migrations, file), 'utf8'))
  .join('\n')

const tables = (Object.values(schema) as unknown[])
  .filter((value): value is PgTable => is(value, PgTable))
  .map(getTableName)
  .sort()

describe('RLS «нікому»', () => {
  it('бачить усі три таблиці схеми', () => {
    expect(tables).toEqual(['disputes', 'evidence', 'reports'])
  })

  it('вмикає RLS на кожній таблиці', () => {
    for (const table of tables) {
      expect(sql, table).toContain(`ALTER TABLE "${table}" ENABLE ROW LEVEL SECURITY;`)
    }
  })

  it('кладе на кожну таблицю RESTRICTIVE-заборону для anon і authenticated', () => {
    for (const table of tables) {
      expect(sql, table).toContain(
        `CREATE POLICY "${table}_deny_all" ON "${table}" ` +
          'AS RESTRICTIVE FOR ALL TO "anon", "authenticated" ' +
          'USING (false) WITH CHECK (false);',
      )
    }
  })

  it('не має жодної політики, яка щось дозволяє', () => {
    const policies = sql.match(/CREATE POLICY[^;]+;/g) ?? []
    expect(policies).toHaveLength(tables.length)
    for (const policy of policies) {
      expect(policy).toContain('AS RESTRICTIVE')
      expect(policy).toContain('USING (false)')
      expect(policy).toContain('WITH CHECK (false)')
    }
  })
})

describe('міграція', () => {
  /**
   * Обмеження живуть у схемі, але тримає їх база — і лише якщо вони доїхали в
   * SQL. Перевірка йде по назвах: перейменоване обмеження — це нове
   * обмеження, і воно має зʼявитись у наступній міграції, а не зникнути.
   */
  it('везе всі обмеження цілісності', () => {
    for (const constraint of [
      'disputes_amount_non_negative',
      'disputes_votes_claimant_non_negative',
      'disputes_votes_respondent_non_negative',
      'disputes_synced_slot_non_negative',
      'disputes_reveal_after_commit',
      'disputes_appeal_after_reveal',
      'disputes_verdict_matches_state',
      'disputes_report_hash_lower_hex',
      'disputes_claimant_claim_hash_lower_hex',
      'disputes_respondent_claim_hash_lower_hex',
      'reports_version_positive',
      'reports_content_hash_lower_hex',
      'evidence_slot_non_negative',
    ]) {
      expect(sql, constraint).toContain(`CONSTRAINT "${constraint}" CHECK`)
    }
  })

  it('дає панелі GIN-індекс, бо її шукають за входженням гаманця', () => {
    expect(sql).toContain('CREATE INDEX "disputes_panel_idx" ON "disputes" USING gin ("panel");')
  })
})
