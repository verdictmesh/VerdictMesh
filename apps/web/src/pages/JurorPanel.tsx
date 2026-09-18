import { ArrowRight } from 'lucide-react'
import { Link } from 'react-router-dom'
import Countdown from '@/components/Countdown'
import PhaseChip from '@/components/PhaseChip'
import Shell from '@/components/Shell'
import { useOrderedHearings } from '@/hooks/useHearingClock'
import { JUROR, PENALTIES, usd } from '@/lib/verdictmesh'

const StandingCell = ({ label, value, note }: { label: string; value: string; note?: string }) => (
  <div className="flex min-w-[140px] flex-col gap-1.5 border-l border-border pl-4 first:border-l-0 first:pl-0">
    <span className="label-xs text-muted-foreground">{label}</span>
    <span className="mono tabular text-[17px] font-medium leading-none text-foreground">
      {value}
    </span>
    {note ? <span className="text-[12px] leading-tight text-muted-foreground">{note}</span> : null}
  </div>
)

const JurorPanel = () => {
  const rows = useOrderedHearings()

  return (
    <Shell>
      <section className="mb-7">
        <div className="mb-3 flex items-baseline justify-between">
          <h1 className="text-[15px] font-semibold tracking-tight text-foreground">
            Juror standing
          </h1>
          <span className="label-xs text-unestablished">{JUROR.seat} · staked juror</span>
        </div>
        <div className="rounded border border-border bg-surface p-4">
          <div className="flex flex-wrap gap-x-8 gap-y-5">
            <StandingCell label="Stake at risk" value={usd(JUROR.stake)} />
            <StandingCell label="Hearings seated" value={String(JUROR.seated)} />
            <StandingCell
              label="Agreement with final verdict"
              value={`${JUROR.agreed} / ${JUROR.pastHearings}`}
              note="past hearings"
            />
          </div>
          <p className="mt-4 border-t border-border pt-3 text-[12.5px] leading-relaxed text-muted-foreground">
            One past hearing was decided against your vote. Cost: {usd(JUROR.disagreementCost)} of
            stake ({PENALTIES.losingSide * 100}%). Missing a reveal costs{' '}
            {PENALTIES.missedReveal * 100}%.
          </p>
        </div>
      </section>

      <section>
        <div className="mb-3 flex items-baseline justify-between">
          <h2 className="text-[15px] font-semibold tracking-tight text-foreground">
            Your hearings
          </h2>
          <span className="label-xs text-muted-foreground">sorted by least time remaining</span>
        </div>

        <ul className="flex flex-col gap-2">
          {rows.map(({ hearing, phase, msLeft, urgent }) => (
            <li key={hearing.id}>
              <Link
                to={`/hearing/${hearing.id}`}
                className="focus-ring group block rounded border border-border bg-surface px-4 py-3.5 transition-colors hover:border-border-strong hover:bg-surface-2"
              >
                <div className="flex items-start justify-between gap-6">
                  <div className="min-w-0">
                    <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
                      <span className="mono text-[12px] font-semibold tracking-[0.1em] text-foreground">
                        {hearing.id}
                      </span>
                      <PhaseChip phase={phase} />
                    </div>
                    <p className="mt-2 truncate text-[14.5px] font-medium leading-snug text-foreground">
                      {hearing.dealParty}
                      <span className="text-muted-foreground"> — {hearing.dealDetail}</span>
                    </p>
                    <div className="mt-2 flex flex-wrap items-baseline gap-x-6 gap-y-1">
                      <span className="mono tabular text-[13px] text-foreground">
                        {usd(hearing.amount)}
                        <span className="ml-1.5 text-[11px] text-muted-foreground">in dispute</span>
                      </span>
                      <span className="text-[12.5px] text-muted-foreground">
                        Opened by <span className="text-foreground">{hearing.openedBy}</span>
                      </span>
                    </div>
                  </div>

                  <div className="flex shrink-0 items-center gap-4">
                    <Countdown msLeft={msLeft} urgent={urgent} />
                    <ArrowRight className="h-4 w-4 text-unestablished transition-colors group-hover:text-foreground" />
                  </div>
                </div>
              </Link>
            </li>
          ))}
        </ul>

        <div className="mt-6 rounded border border-border bg-surface px-4 py-3">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <span className="label-xs text-muted-foreground">Recently settled</span>
              <p className="mt-1.5 text-[13.5px] text-foreground">
                VM-1039 — Tessera Market, order #8813
                <span className="text-muted-foreground"> · verdict recorded, escrow released</span>
              </p>
            </div>
            <Link
              to="/settlement/VM-1039"
              className="focus-ring label-xs rounded-sm border border-border-strong px-2.5 py-1.5 text-foreground transition-colors hover:bg-surface-2"
            >
              Open receipt
            </Link>
          </div>
        </div>
      </section>
    </Shell>
  )
}

export default JurorPanel
