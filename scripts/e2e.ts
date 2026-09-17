/**
 * Що саме міряє наскрізний прогін — окремо від того, як він ходить у мережу.
 *
 * Числа з devnet приходять один раз і повільно: цикл не можна перезапустити
 * двадцять разів, щоб перевірити, чи правильно порахований бюджет. Тому рішення
 * «пройшов / не пройшов» живе тут, під тестами, а мережевий скрипт лише
 * приносить сюди заміри.
 */

export type Ballot = 'claimant' | 'respondent'
export type Outcome = Ballot | 'statusQuo'

/** Те, з чого складається очікування: розмір панелі, кворум і три вікна. */
export interface Policy {
  panelSize: number
  quorum: number
  commitWindow: number
  revealWindow: number
  appealWindow: number
}

/**
 * Чим має закінчитись підрахунок при таких розкритих голосах — та сама
 * арифметика, що в `tally`. Потрібна, щоб прогін звіряв вердикт із тим, за що
 * панель насправді голосувала: інакше «цикл пройшов за 3 хвилини» лишалось би
 * правдою і тоді, коли вердикт протилежний голосам.
 *
 * `escalate` — не вердикт, а те, що буває замість нього: недобір кворуму або
 * рівність (`FR-027`).
 */
export function expectedOutcome(revealed: readonly Ballot[], policy: Policy): Outcome | 'escalate' {
  if (revealed.length > policy.panelSize) {
    throw new Error(`More votes than the panel holds: ${revealed.length} of ${policy.panelSize}`)
  }

  if (revealed.length < policy.quorum) return 'escalate'

  const claimant = revealed.filter((ballot) => ballot === 'claimant').length
  const respondent = revealed.length - claimant

  if (claimant === respondent) return 'escalate'
  return claimant > respondent ? 'claimant' : 'respondent'
}

/**
 * Скільки секунд цикл не може не тривати. Сума трьох вікон, і вікно апеляції
 * входить у неї нарівні з рештою: виплату не виконати, поки воно не закрилось.
 *
 * Це розходиться з бюджетом у `docs/PLAN.md`, де на цикл покладено ~2.5 хвилини
 * — там пораховані подання, розкриття і звіт, але не апеляція. Обидва числа
 * лишаються під `SC-001` (5 хвилин), тож розходження не змінює висновку, але
 * запас удвічі менший, ніж записано.
 */
export function cycleFloorSeconds(policy: Policy): number {
  return policy.commitWindow + policy.revealWindow + policy.appealWindow
}

/**
 * Комісії міряються в лампортах, а `SC-002` — у доларах, тож ціна SOL входить у
 * вимір явним аргументом. Нуль і від'ємне відхиляються: за такою ціною будь-який
 * прогін вкладається в будь-який бюджет, і критерій перестав би щось означати.
 */
export function usdOfLamports(lamports: bigint, solPriceUsd: number): number {
  if (!(solPriceUsd > 0)) {
    throw new Error(`SOL price must be positive, got ${solPriceUsd}`)
  }

  return (Number(lamports) / 1e9) * solPriceUsd
}

/** Один цикл «спір → голосування → виплата», як його побачив прогін. */
export interface Run {
  dispute: string
  elapsedMs: number
  feeLamports: bigint
  outcome: Outcome
  expected: Outcome
  /** Усе, що прогін мусив зробити руками. Порожньо — значить, автоматично. */
  interventions: readonly string[]
}

export interface Criteria {
  /** `SC-001`: повний цикл, секунди. */
  cycleSeconds: number
  /** `SC-002`: сумарні комісії одного циклу, долари. */
  cycleUsd: number
  solPriceUsd: number
}

export interface Report {
  sc001: { passed: boolean; slowestMs: number; budgetMs: number }
  sc002: { passed: boolean; worstUsd: number; budgetUsd: number }
  sc005: { passed: boolean; automatic: number; total: number; needed: string[] }
}

/**
 * Три критерії міряються по найгіршому циклу, а не по середньому. Середнє
 * ховає рівно той випадок, заради якого критерій написаний: один цикл із
 * двадцяти, що не вклався, — це вже «не вкладаємось», а не «в межах похибки».
 */
export function measure(runs: readonly Run[], criteria: Criteria): Report {
  if (runs.length === 0) {
    throw new Error('No cycles to measure: a run with no disputes proves nothing')
  }

  const budgetMs = criteria.cycleSeconds * 1000
  const slowestMs = Math.max(...runs.map((run) => run.elapsedMs))
  const worstUsd = Math.max(
    ...runs.map((run) => usdOfLamports(run.feeLamports, criteria.solPriceUsd)),
  )

  // Вердикт, що розійшовся з голосами панелі, теж вимагає рук: розгляд, який дав
  // не той результат, довелося б виправляти, і питання лише в тому, чи хтось
  // помітив.
  const reasons = (run: Run): string[] => [
    ...run.interventions,
    ...(run.outcome === run.expected
      ? []
      : [`${run.dispute}: verdict ${run.outcome} where the panel voted ${run.expected}`]),
  ]

  const needed = runs.flatMap(reasons)

  return {
    sc001: { passed: slowestMs <= budgetMs, slowestMs, budgetMs },
    sc002: { passed: worstUsd <= criteria.cycleUsd, worstUsd, budgetUsd: criteria.cycleUsd },
    sc005: {
      passed: needed.length === 0,
      automatic: runs.filter((run) => reasons(run).length === 0).length,
      total: runs.length,
      needed,
    },
  }
}
