import {
  disputeState as disputeStateContract,
  disputeView,
  verdict as verdictContract,
} from '@verdictmesh/shared'
import { getTableConfig } from 'drizzle-orm/pg-core'
import { describe, expect, it } from 'vitest'
import { disputeStateEnum, disputes, evidence, reports, verdictEnum } from './schema.js'

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
  it('має колонку під кожне поле DisputeView, крім settled', () => {
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
      'commitDeadline',
      'revealDeadline',
      'appealDeadline',
      'escalated',
      'verdict',
    ])
    expect(Object.keys(disputeView.shape)).toHaveLength(covered.length + 1)
  })

  /**
   * `settled` колонки не має, і це не пропуск. Поля `Dispute.settled` на вісі
   * більше немає — T020 прибрав його як другий запис того, що вже сказано
   * станом. «Виконано» в сенсі `FR-020` — це виплата **в ескроу**, подій якого
   * watcher (T027) не слухає взагалі: програма там чужа. Тож заповнити цю
   * колонку сьогодні нічим, і порожня вона брехала б переконливіше за
   * відсутню. Питання вирішують T031 і T033 — або `DisputeView` втрачає поле,
   * або зʼявляється джерело, з якого його беруть.
   */
  it('не має колонки settled, поки немає джерела для неї', () => {
    expect(disputeColumns.has('settled')).toBe(false)
    expect(Object.keys(disputeView.shape)).toContain('settled')
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

  it('привʼязують і звіт, і доказ до наявного спору', () => {
    for (const table of [reports, evidence]) {
      const [foreignKey] = getTableConfig(table).foreignKeys
      const reference = foreignKey?.reference()
      expect(reference?.foreignTable, getTableConfig(table).name).toBe(disputes)
      expect(reference?.foreignColumns.map((column) => column.name)).toEqual(['pda'])
    }
  })
})
