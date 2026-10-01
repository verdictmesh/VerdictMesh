import type { DisputeView } from '@verdictmesh/shared'
import { ArrowRight } from 'lucide-react'
import { Link } from 'react-router-dom'
import Countdown from '@/components/Countdown'
import { Failure, Loading } from '@/components/LoadState'
import Shell from '@/components/Shell'
import StageChip from '@/components/StageChip'
import {
  formatAgo,
  formatAmount,
  OPEN_STAGES,
  type Stage,
  shortenMiddle,
  stageDeadline,
  stageOf,
  VERDICT_LABEL,
} from '@/lib/dispute'
import { useDisputes, useNow } from '@/lib/queries'

/** Open windows first, soonest deadline first; the rest newest first, as served. */
function ordered(disputes: readonly DisputeView[], now: number) {
  const rows = disputes.map((dispute) => {
    const stage = stageOf(dispute, now)
    return { dispute, stage, deadline: stageDeadline(dispute, stage) }
  })
  const open = rows
    .filter((row) => OPEN_STAGES.has(row.stage))
    .sort((a, b) => (a.deadline ?? 0) - (b.deadline ?? 0))
  return [...open, ...rows.filter((row) => !OPEN_STAGES.has(row.stage))]
}

const Row = ({ dispute, stage, now }: { dispute: DisputeView; stage: Stage; now: number }) => {
  const deadline = stageDeadline(dispute, stage)
  const msLeft = deadline === null ? null : (deadline - now) * 1000

  return (
    <Link
      to={`/hearing/${dispute.pda}`}
      className="focus-ring group block rounded border border-border bg-surface px-4 py-3.5 transition-colors hover:border-border-strong hover:bg-surface-2"
    >
      <div className="flex items-start justify-between gap-6">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
            <span className="mono text-[12px] font-semibold tracking-[0.06em] text-foreground">
              {shortenMiddle(dispute.pda, 6, 6)}
            </span>
            <StageChip stage={stage} />
          </div>
          <div className="mt-2 flex flex-wrap items-baseline gap-x-6 gap-y-1">
            <span className="mono tabular text-[13px] text-foreground">
              {formatAmount(dispute.amount)}
              <span className="ml-1.5 text-[11px] text-muted-foreground">in dispute</span>
            </span>
            <span className="text-[12.5px] text-muted-foreground">
              Opened {formatAgo(dispute.openedAt, now)}
            </span>
            {dispute.verdict ? (
              <span className="text-[12.5px] text-muted-foreground">
                {VERDICT_LABEL[dispute.verdict]}
              </span>
            ) : null}
          </div>
        </div>

        <div className="flex shrink-0 items-center gap-4">
          {msLeft === null ? null : <Countdown msLeft={msLeft} urgent={msLeft < 15_000} />}
          <ArrowRight className="h-4 w-4 text-unestablished transition-colors group-hover:text-foreground" />
        </div>
      </div>
    </Link>
  )
}

const Hearings = () => {
  const now = useNow()
  const { data, error } = useDisputes()

  return (
    <Shell>
      <section>
        <div className="mb-3 flex flex-wrap items-baseline justify-between gap-2">
          <h1 className="text-[15px] font-semibold tracking-tight text-foreground">Hearings</h1>
          <span className="label-xs text-muted-foreground">open windows first · then newest</span>
        </div>
        <p className="mb-4 max-w-[680px] text-[13px] leading-relaxed text-muted-foreground">
          Every dispute opened through VerdictMesh on Solana devnet, read from the chain. Anyone can
          follow a hearing without a wallet; only panel members vote.
        </p>

        {error ? (
          <Failure error={error} />
        ) : !data ? (
          <Loading what="hearings" />
        ) : data.length === 0 ? (
          <p className="text-[13.5px] text-muted-foreground">No hearings yet.</p>
        ) : (
          <ul className="flex flex-col gap-2">
            {ordered(data, now).map(({ dispute, stage }) => (
              <li key={dispute.pda}>
                <Row dispute={dispute} stage={stage} now={now} />
              </li>
            ))}
          </ul>
        )}
      </section>
    </Shell>
  )
}

export default Hearings
