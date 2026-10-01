import { useQuery } from '@tanstack/react-query'
import type { DisputeView, ReportResponse } from '@verdictmesh/shared'
import { useEffect, useState } from 'react'
import { type DisputeFilter, getDispute, getReport, listDisputes } from './api'
import { OPEN_STAGES, stageOf } from './dispute'

/**
 * Polling, not the `/events` stream: the mirror itself catches up with the
 * chain in seconds, and a few-second poll of one row is cheaper than holding a
 * connection open on a free host that sleeps. The interval follows the stage —
 * a dispute in a voting window moves every few seconds, a settled one never.
 */

const nowSec = () => Math.floor(Date.now() / 1000)

const pollFor = (dispute: DisputeView | undefined) => {
  if (!dispute) return 10_000
  const stage = stageOf(dispute, nowSec())
  if (OPEN_STAGES.has(stage) || stage === 'tally') return 5_000
  if (stage === 'settled') return false
  return 15_000
}

export function useDisputes(filter: DisputeFilter = {}) {
  return useQuery({
    queryKey: ['disputes', filter],
    queryFn: ({ signal }) => listDisputes(filter, signal),
    refetchInterval: 15_000,
  })
}

export function useDispute(pda: string) {
  return useQuery({
    queryKey: ['dispute', pda],
    queryFn: ({ signal }) => getDispute(pda, signal),
    refetchInterval: (query) => pollFor(query.state.data),
  })
}

/** A report that is ready is final (`FR-017`); a missing one may still come. */
const reportPoll = (report: ReportResponse | undefined) => {
  if (!report) return 10_000
  if (report.status === 'ready') return false
  if (report.status === 'unavailable' && report.final) return false
  return 15_000
}

export function useReport(pda: string) {
  return useQuery({
    queryKey: ['report', pda],
    queryFn: ({ signal }) => getReport(pda, signal),
    refetchInterval: (query) => reportPoll(query.state.data),
  })
}

/** Unix seconds, ticking once a second — for countdowns. */
export function useNow(): number {
  const [now, setNow] = useState(nowSec)
  useEffect(() => {
    const timer = setInterval(() => setNow(nowSec()), 1_000)
    return () => clearInterval(timer)
  }, [])
  return now
}
