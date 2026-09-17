import { describe, expect, it } from 'vitest'
import {
  cycleFloorSeconds,
  expectedOutcome,
  measure,
  usdOfLamports,
  type Run,
} from './e2e.js'

const policy = {
  panelSize: 3,
  quorum: 2,
  commitWindow: 60,
  revealWindow: 60,
  appealWindow: 90,
}

describe('expectedOutcome', () => {
  it('gives the verdict to the majority of the revealed votes', () => {
    expect(expectedOutcome(['claimant', 'claimant', 'respondent'], policy)).toBe('claimant')
    expect(expectedOutcome(['respondent', 'respondent', 'claimant'], policy)).toBe('respondent')
  })

  it('counts a panel that reached exactly the quorum', () => {
    expect(expectedOutcome(['claimant', 'claimant'], policy)).toBe('claimant')
  })

  /**
   * Кворум рахується за розкритими голосами, а не за розміром панелі: мовчання
   * двох із трьох лишає підрахунок без більшості, і спір іде на розширену
   * панель, а не отримує вердикт від того, хто встиг.
   */
  it('escalates a panel that stayed below the quorum', () => {
    expect(expectedOutcome(['claimant'], policy)).toBe('escalate')
    expect(expectedOutcome([], policy)).toBe('escalate')
  })

  it('escalates a tie even when the quorum was reached', () => {
    expect(expectedOutcome(['claimant', 'respondent'], policy)).toBe('escalate')
  })

  it('refuses a panel larger than the policy allows', () => {
    expect(() => expectedOutcome(['claimant', 'claimant', 'claimant', 'claimant'], policy)).toThrow(
      /panel/i,
    )
  })
})

describe('cycleFloorSeconds', () => {
  /**
   * Нижня межа циклу — сума трьох вікон, і вікно апеляції входить у неї. Виплату
   * не можна виконати, поки воно не закрилось (`settle_milestone` перевіряє
   * дедлайн), тож бюджет `SC-001`, порахований без нього, недорахував би 90
   * секунд із п'яти хвилин.
   */
  it('counts the appeal window, because the payout cannot precede it', () => {
    expect(cycleFloorSeconds(policy)).toBe(210)
  })

  it('grows with every window it waits on', () => {
    expect(cycleFloorSeconds({ ...policy, appealWindow: 0 })).toBe(120)
  })
})

describe('usdOfLamports', () => {
  it('converts at the given price', () => {
    expect(usdOfLamports(5_000_000n, 200)).toBeCloseTo(1, 10)
  })

  it('is zero for no fees at all', () => {
    expect(usdOfLamports(0n, 200)).toBe(0)
  })

  it('refuses a price that would make every cycle look free', () => {
    expect(() => usdOfLamports(1n, 0)).toThrow(/price/i)
    expect(() => usdOfLamports(1n, -1)).toThrow(/price/i)
  })
})

const run = (over: Partial<Run> = {}): Run => ({
  dispute: 'dispute-1',
  elapsedMs: 220_000,
  feeLamports: 60_000n,
  outcome: 'claimant',
  expected: 'claimant',
  interventions: [],
  ...over,
})

describe('measure', () => {
  const criteria = { cycleSeconds: 300, cycleUsd: 0.1, solPriceUsd: 200 }

  it('passes a run where every cycle stayed inside every budget', () => {
    const report = measure([run(), run({ dispute: 'dispute-2' })], criteria)

    expect(report.sc001.passed).toBe(true)
    expect(report.sc002.passed).toBe(true)
    expect(report.sc005.passed).toBe(true)
    expect(report.sc005.automatic).toBe(2)
  })

  it('reports the slowest cycle, not the average', () => {
    const report = measure([run(), run({ elapsedMs: 400_000 })], criteria)

    expect(report.sc001.slowestMs).toBe(400_000)
    expect(report.sc001.passed).toBe(false)
  })

  /**
   * `SC-002` міряє вартість **циклу**, а не всього прогону: 20 спорів по три
   * центи це дешевий протокол і дорогий прогін, і сума сказала б протилежне до
   * правди.
   */
  it('measures the cost of a cycle rather than of the whole run', () => {
    const report = measure(
      [run({ feeLamports: 40_000n }), run({ feeLamports: 60_000n })],
      criteria,
    )

    expect(report.sc002.worstUsd).toBeCloseTo(usdOfLamports(60_000n, 200), 10)
    expect(report.sc002.passed).toBe(true)
  })

  it('fails the fee criterion on the single cycle that broke it', () => {
    const report = measure([run(), run({ feeLamports: 200_000_000n })], criteria)

    expect(report.sc002.passed).toBe(false)
  })

  /**
   * Втручання рахується як провал незалежно від того, чим скінчився спір: `SC-005`
   * міряє саме автоматичність, і розгляд, доведений руками, її не має.
   */
  it('counts a cycle that needed a hand as not automatic', () => {
    const report = measure([run(), run({ interventions: ['tally never landed'] })], criteria)

    expect(report.sc005.passed).toBe(false)
    expect(report.sc005.automatic).toBe(1)
    expect(report.sc005.needed).toEqual(['tally never landed'])
  })

  /**
   * Вердикт, що розійшовся з голосами панелі, — не повільний цикл і не дорогий,
   * а неправильний. Він теж не проходить `SC-005`: розгляд, який дав не той
   * результат, довелося б виправляти руками, і питання лише в тому, чи хтось
   * помітив.
   */
  it('treats a verdict that contradicts the panel as an intervention', () => {
    const report = measure([run({ outcome: 'respondent', expected: 'claimant' })], criteria)

    expect(report.sc005.passed).toBe(false)
    expect(report.sc005.needed[0]).toMatch(/verdict/i)
  })

  it('refuses to call an empty run a success', () => {
    expect(() => measure([], criteria)).toThrow(/no cycles/i)
  })
})
