import { ChevronLeft } from 'lucide-react'
import { Link, useParams } from 'react-router-dom'
import Countdown from '@/components/Countdown'
import EvidenceReport from '@/components/EvidenceReport'
import PhaseChip from '@/components/PhaseChip'
import Shell from '@/components/Shell'
import VotePanel from '@/components/VotePanel'
import { useHearingClock } from '@/hooks/useHearingClock'
import { getHearing, REPORTS, usd } from '@/lib/verdictmesh'

const HearingJuror = () => {
  const { id } = useParams()
  const hearing = getHearing(id)
  const { phase, msLeft, urgent } = useHearingClock(id)

  if (!hearing) {
    return (
      <Shell>
        <p className="text-[13.5px] text-muted-foreground">
          No such hearing.{' '}
          <Link to="/" className="text-foreground underline">
            Back to the juror panel
          </Link>
        </p>
      </Shell>
    )
  }

  const report = REPORTS[hearing.id]

  return (
    <Shell>
      <div className="mb-5 flex flex-wrap items-start justify-between gap-4 border-b border-border pb-4">
        <div className="min-w-0">
          <Link
            to="/"
            className="focus-ring label-xs inline-flex items-center gap-1 text-muted-foreground transition-colors hover:text-foreground"
          >
            <ChevronLeft className="h-3 w-3" /> Juror panel
          </Link>
          <div className="mt-2.5 flex flex-wrap items-center gap-x-3 gap-y-2">
            <h1 className="mono text-[15px] font-semibold tracking-[0.1em] text-foreground">
              {hearing.id}
            </h1>
            <PhaseChip phase={phase} />
            <span className="label-xs text-unestablished">juror view</span>
          </div>
          <p className="mt-2 text-[14.5px] leading-snug text-foreground">
            {hearing.dealParty}
            <span className="text-muted-foreground"> — {hearing.dealDetail}</span>
          </p>
          <p className="mono tabular mt-1.5 text-[13px] text-foreground">
            {usd(hearing.amount)}
            <span className="ml-1.5 text-[11px] text-muted-foreground">
              in dispute · opened by {hearing.openedBy}
            </span>
          </p>
        </div>
        <div className="flex flex-col items-end gap-3">
          <Countdown msLeft={msLeft} urgent={urgent} size="lg" />
          <Link
            to={`/hearing/${hearing.id}/party`}
            className="focus-ring label-xs rounded-sm border border-border px-2.5 py-1.5 text-muted-foreground transition-colors hover:border-border-strong hover:text-foreground"
          >
            Open party view
          </Link>
        </div>
      </div>

      {/* THE CLAIM */}
      <section className="mb-7">
        <div className="mb-2.5 flex items-baseline justify-between">
          <h2 className="text-[15px] font-semibold tracking-tight text-foreground">The claim</h2>
          <span className="label-xs text-muted-foreground">
            each side wrote only its own sentence
          </span>
        </div>
        <div className="grid gap-2 md:grid-cols-2">
          {hearing.sides.map((side) => (
            <div key={side.role} className="rounded border border-border bg-surface p-4">
              <div className="flex items-baseline justify-between">
                <span className="label-xs text-foreground">{side.role}</span>
                <span className="label-xs text-unestablished">own position</span>
              </div>
              <p className="mt-2.5 text-[14.5px] font-medium leading-relaxed text-foreground">
                “{side.position}”
              </p>
              <p className="mt-2.5 border-t border-border pt-2.5 text-[11.5px] leading-snug text-unestablished">
                Submitted by the {side.role.toLowerCase()}. Neither side can edit, summarise or
                answer for the other.
              </p>
            </div>
          ))}
        </div>
      </section>

      {/* THE EVIDENCE REPORT */}
      <section className="mb-7">
        <div className="mb-3 flex flex-wrap items-baseline justify-between gap-2">
          <h2 className="text-[15px] font-semibold tracking-tight text-foreground">
            Evidence report
          </h2>
          <span className="label-xs text-muted-foreground">
            machine-assembled from on-chain history
          </span>
        </div>

        <div className="rounded border border-border bg-surface p-4">
          {report ? (
            <EvidenceReport report={report} />
          ) : (
            <p className="text-[13.5px] leading-relaxed text-muted-foreground">
              The assembled report for this hearing is not part of this demo. VM-1042 carries the
              full report.
            </p>
          )}

          <div className="mt-6 border-t border-border pt-3.5">
            <span className="label-xs text-muted-foreground">What the money does</span>
            <p className="mt-2 text-[13.5px] leading-relaxed text-foreground">{hearing.payout}</p>
          </div>
        </div>
      </section>

      {/* THE VOTE */}
      <section>
        <div className="mb-3 flex flex-wrap items-baseline justify-between gap-2">
          <h2 className="text-[15px] font-semibold tracking-tight text-foreground">Your vote</h2>
          <span className="label-xs text-muted-foreground">two steps · 60s each</span>
        </div>
        <VotePanel hearing={hearing} phase={phase} msLeft={msLeft} urgent={urgent} />
      </section>
    </Shell>
  )
}

export default HearingJuror
