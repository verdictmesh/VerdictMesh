import {
  disputeState as disputeStateContract,
  disputeView,
  verdict as verdictContract,
} from '@verdictmesh/shared'
import { getTableConfig } from 'drizzle-orm/pg-core'
import { describe, expect, it } from 'vitest'
import {
  disputeStateEnum,
  disputes,
  evidence,
  reports,
  settlements,
  verdictEnum,
} from './schema.js'

const columns = (table: Parameters<typeof getTableConfig>[0]) =>
  new Map(getTableConfig(table).columns.map((column) => [column.name, column]))

const snake = (name: string) => name.replace(/[A-Z]/g, (letter) => `_${letter.toLowerCase()}`)

describe('enum бази і enum контракту', () => {
  it('однаково перелічує стани спору', () => {
    expect([...disputeStateEnum.enumValues]).toEqual([...disputeStateContract.options])
  })

  it('однаково перелічує вердикти', () => {
    expect([...verdictEnum.enumValues]).toEqual([...verdictContract.options])
  })
})

describe('кеш спорів', () => {
  const disputeColumns = columns(disputes)

  /**
   * Сенс таблиці — віддати `DisputeView` одним запитом, без походу в ланцюг:
   * `SC-010` дає першому екрану панелі дві секунди, а RPC на кожен рядок цього
   * не залишає. Тому поле контракту без колонки — це провалений бюджет, а не
   * дрібниця стилю.
   */
  it('має колонку під кожне поле DisputeView, крім settlement', () => {
    const covered = Object.keys(disputeView.shape).filter((field) =>
      disputeColumns.has(snake(field)),
    )

    // Список, а не «порожній масив невідповідностей»: перевірка, яка звелась
    // до порівняння двох порожніх множин, зеленіє й тоді, коли контракт
    // прочитано неправильно.
    expect(covered).toEqual([
      'pda',
      'integrator',
      'escrowRef',
      'claimant',
      'respondent',
      'amount',
      'state',
      'panel',
      'reportHash',
      'openedAt',
      'commitDeadline',
      'revealDeadline',
      'appealDeadline',
      'votesClaimant',
      'votesRespondent',
      'escalated',
      'verdict',
    ])
    expect(Object.keys(disputeView.shape)).toHaveLength(covered.length + 1)
  })

  /**
   * `settlement` lives in its own table (T033). The mirror upserts every column
   * from each snapshot of the `Dispute` account, which knows nothing of the
   * escrow: a column here would be wiped by the next rewrite.
   */
  it('keeps settlement out of the mirror', () => {
    expect(disputeColumns.has('settled')).toBe(false)
    expect(disputeColumns.has('settlement_signature')).toBe(false)
    expect(Object.keys(disputeView.shape)).toContain('settlement')
  })

  it('тримає суму як цілий u64, а не як int8', () => {
    expect(disputeColumns.get('amount')?.getSQLType()).toBe('numeric(20, 0)')
  })

  it('тримає дедлайни й слот як bigint', () => {
    for (const name of [
      'opened_at',
      'commit_deadline',
      'reveal_deadline',
      'appeal_deadline',
      'synced_slot',
    ]) {
      expect(disputeColumns.get(name)?.getSQLType(), name).toBe('bigint')
    }
  })

  it('тримає всі відбитки як char(64)', () => {
    for (const name of ['report_hash', 'claimant_claim_hash', 'respondent_claim_hash']) {
      expect(disputeColumns.get(name)?.getSQLType(), name).toBe('char(64)')
    }
  })

  /** Відбиток зʼявляється лише після `attest_report`; до того його немає. */
  it('дозволяє порожній відбиток звіту і вимагає обидва відбитки позицій', () => {
    expect(disputeColumns.get('report_hash')?.notNull).toBe(false)
    expect(disputeColumns.get('claimant_claim_hash')?.notNull).toBe(true)
    expect(disputeColumns.get('respondent_claim_hash')?.notNull).toBe(true)
  })
})

describe('звіти і докази', () => {
  it('версіонують звіт замість перезапису', () => {
    const primaryKey = getTableConfig(reports).primaryKeys[0]
    expect(primaryKey?.columns.map((column) => column.name)).toEqual(['dispute_pda', 'version'])
  })

  it('не дають двох доказів з одного джерела на один спір', () => {
    const primaryKey = getTableConfig(evidence).primaryKeys[0]
    expect(primaryKey?.columns.map((column) => column.name)).toEqual(['dispute_pda', 'source'])
  })

  it('привʼязують звіт, доказ і виконання до наявного спору', () => {
    for (const table of [reports, evidence, settlements]) {
      const [foreignKey] = getTableConfig(table).foreignKeys
      const reference = foreignKey?.reference()
      expect(reference?.foreignTable, getTableConfig(table).name).toBe(disputes)
      expect(reference?.foreignColumns.map((column) => column.name)).toEqual(['pda'])
    }
  })
})

describe('settlements', () => {
  it('holds at most one settlement per dispute', () => {
    const settlementColumns = columns(settlements)
    expect(settlementColumns.get('dispute_pda')?.primary).toBe(true)
    expect(settlementColumns.get('signature')?.notNull).toBe(true)
    expect(settlementColumns.get('slot')?.getSQLType()).toBe('bigint')
  })
})
