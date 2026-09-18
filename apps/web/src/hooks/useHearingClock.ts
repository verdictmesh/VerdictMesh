import { useEffect, useState } from 'react'
import { HEARINGS, type Hearing, PHASE_DURATION_MS, type Phase } from '@/lib/verdictmesh'

/**
 * Shared, purely local clock for the demo hearings.
 * Deadlines are seeded once from the hardcoded seconds and then tick in real time,
 * so every screen agrees on the same countdown.
 *
 * The demo is a loop, not a single run. A hearing that reaches `settled` reopens at
 * the phase it began in, and the hearing the visitor is meant to vote in holds its
 * commit window open until a vote is sealed: somebody who spends a minute reading
 * the panel must still be able to cast one.
 */

/** The hearing the juror screens invite the visitor to vote in. */
export const VOTING_HEARING_ID = 'VM-1042'

/** Beyond this many expiries in one pass the clock is stale (a slept tab), not late. */
const MAX_CATCH_UP_STEPS = 64

const NEXT_PHASE: Record<Phase, Phase> = {
  commit: 'reveal',
  reveal: 'appeal',
  appeal: 'settled',
  settled: 'settled',
}

export interface ClockState {
  phase: Phase
  deadline: number
}

/** The hearing as it opened: its seeded phase, running for its seeded duration. */
export function openingState(hearing: Hearing, from: number): ClockState {
  return { phase: hearing.phase, deadline: from + hearing.secondsLeft * 1000 }
}

/**
 * One expiry: what the hearing does when its current window runs out.
 * Pure, so the loop can be tested without a clock.
 */
export function afterDeadline(hearing: Hearing, state: ClockState, sealed: boolean): ClockState {
  // The vote the visitor is meant to cast. The window reopens instead of closing,
  // for as long as nothing is sealed; after that the phases mean what they say.
  if (hearing.id === VOTING_HEARING_ID && state.phase === 'commit' && !sealed) {
    return { phase: 'commit', deadline: state.deadline + PHASE_DURATION_MS.commit }
  }

  const next = NEXT_PHASE[state.phase]

  // Settled is where the demo would end. It reopens instead.
  if (next === 'settled') return openingState(hearing, state.deadline)

  return { phase: next, deadline: state.deadline + PHASE_DURATION_MS[next] }
}

/** Every expiry between `state` and `now`, at most one slept tab's worth. */
export function advanceClock(
  hearing: Hearing,
  state: ClockState,
  now: number,
  sealed: boolean,
): ClockState {
  let current = state

  for (let step = 0; now >= current.deadline; step += 1) {
    if (step >= MAX_CATCH_UP_STEPS) return openingState(hearing, now)
    current = afterDeadline(hearing, current, sealed)
  }

  return current
}

const hearingsById = new Map(HEARINGS.map((hearing) => [hearing.id, hearing]))
const clocks = new Map<string, ClockState>()
const sealedVotes = new Set<string>()

for (const hearing of HEARINGS) {
  clocks.set(hearing.id, openingState(hearing, Date.now()))
}

/** Called once the visitor seals a vote: that hearing stops waiting for them. */
export function sealVote(id: string) {
  sealedVotes.add(id)
}

function advance(now: number) {
  for (const [id, state] of clocks) {
    const hearing = hearingsById.get(id)
    if (!hearing) continue
    clocks.set(id, advanceClock(hearing, state, now, sealedVotes.has(id)))
  }
}

let listeners = 0
let timer: ReturnType<typeof setInterval> | null = null
const subscribers = new Set<() => void>()

function subscribe(fn: () => void) {
  subscribers.add(fn)
  listeners += 1
  if (!timer) {
    timer = setInterval(() => {
      advance(Date.now())
      for (const notify of subscribers) notify()
    }, 250)
  }
  return () => {
    subscribers.delete(fn)
    listeners -= 1
    if (listeners <= 0 && timer) {
      clearInterval(timer)
      timer = null
    }
  }
}

export interface HearingClock {
  phase: Phase
  msLeft: number
  /** true once under 20 seconds remain in the window */
  urgent: boolean
}

function read(state: ClockState, now: number): HearingClock {
  const msLeft = state.phase === 'settled' ? 0 : Math.max(0, state.deadline - now)
  return { phase: state.phase, msLeft, urgent: msLeft > 0 && msLeft <= 20_000 }
}

export function useHearingClock(id: string | undefined): HearingClock {
  const [, tick] = useState(0)

  useEffect(() => subscribe(() => tick((n) => n + 1)), [])

  const state = id ? clocks.get(id) : undefined
  if (!state) return { phase: 'settled', msLeft: 0, urgent: false }

  return read(state, Date.now())
}

/** Sorted by least time remaining, settled hearings last. */
export function useOrderedHearings() {
  const [, tick] = useState(0)
  useEffect(() => subscribe(() => tick((n) => n + 1)), [])

  const now = Date.now()
  return HEARINGS.map((hearing) => {
    const state = clocks.get(hearing.id) ?? openingState(hearing, now)
    return { hearing, ...read(state, now) }
  }).sort((a, b) => {
    if (a.phase === 'settled' && b.phase !== 'settled') return 1
    if (b.phase === 'settled' && a.phase !== 'settled') return -1
    return a.msLeft - b.msLeft
  })
}
