import { describe, expect, it } from 'vitest'
import { HEARINGS, type Hearing, PHASE_DURATION_MS, type Phase } from '@/lib/verdictmesh'
import {
  advanceClock,
  afterDeadline,
  type ClockState,
  openingState,
  VOTING_HEARING_ID,
} from './useHearingClock'

function hearing(id: string): Hearing {
  const found = HEARINGS.find((h) => h.id === id)
  if (!found) throw new Error(`no such demo hearing: ${id}`)
  return found
}

/** Walk a hearing through `steps` expiries, collecting the phase after each one. */
function walk(subject: Hearing, steps: number, sealed = false): Phase[] {
  let state = openingState(subject, 0)
  const seen: Phase[] = []
  for (let i = 0; i < steps; i += 1) {
    state = afterDeadline(subject, state, sealed)
    seen.push(state.phase)
  }
  return seen
}

const voting = hearing(VOTING_HEARING_ID)
const other = hearing('VM-1041')

describe('the hearing the visitor votes in', () => {
  it('opens in the commit window', () => {
    expect(openingState(voting, 0).phase).toBe('commit')
  })

  it('reopens the commit window instead of closing it while nothing is sealed', () => {
    expect(walk(voting, 6)).toEqual(['commit', 'commit', 'commit', 'commit', 'commit', 'commit'])
  })

  it('keeps the countdown meaningful across a reopen', () => {
    const opened = openingState(voting, 0)
    const reopened = afterDeadline(voting, opened, false)
    expect(reopened.deadline - opened.deadline).toBe(PHASE_DURATION_MS.commit)
  })

  it('lets the phases run once a vote is sealed', () => {
    expect(walk(voting, 3, true)).toEqual(['reveal', 'appeal', 'commit'])
  })
})

describe('the other hearings', () => {
  it('are never held: an expiry always moves them on', () => {
    expect(afterDeadline(other, openingState(other, 0), false).phase).toBe('appeal')
  })

  it('reopen at the phase they began in, for their original duration', () => {
    const opened = openingState(other, 0)
    const appeal = afterDeadline(other, opened, false)
    const reopened = afterDeadline(other, appeal, false)

    expect(reopened.phase).toBe(other.phase)
    expect(reopened.deadline - appeal.deadline).toBe(other.secondsLeft * 1000)
  })
})

describe('the demo never runs out', () => {
  it.each(HEARINGS.map((h) => h.id))('%s never comes to rest in settled', (id) => {
    const subject = hearing(id)
    expect(walk(subject, 40)).not.toContain('settled')
  })

  it('leaves every hearing with a live deadline after a long walk', () => {
    for (const subject of HEARINGS) {
      const now = 30 * 60_000
      const state = advanceClock(subject, openingState(subject, 0), now, false)
      expect(state.deadline).toBeGreaterThan(now)
    }
  })
})

describe('a tab that slept', () => {
  it('resyncs from now rather than replaying hours of windows', () => {
    const now = 24 * 60 * 60_000
    const state = advanceClock(voting, openingState(voting, 0), now, false)
    expect(state).toEqual(openingState(voting, now))
  })

  it('leaves a clock that has not expired alone', () => {
    const opened: ClockState = openingState(voting, 0)
    expect(advanceClock(voting, opened, opened.deadline - 1, false)).toEqual(opened)
  })
})
