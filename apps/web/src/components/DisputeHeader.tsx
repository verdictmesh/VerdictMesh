import type { DisputeView } from '@verdictmesh/shared'
import { ChevronLeft } from 'lucide-react'
import type { ReactNode } from 'react'
import { Link } from 'react-router-dom'
import Countdown from '@/components/Countdown'
import HashRef from '@/components/HashRef'
import StageChip from '@/components/StageChip'
import {
  explorerAddress,
  formatAgo,
  formatAmount,
  STAGE_LABEL,
  stageDeadline,
  stageOf,
} from '@/lib/dispute'

interface DisputeHeaderProps {
  dispute: DisputeView
  now: number
  view: string
  back: { to: string; label: string }
  aside?: ReactNode
}

/** The dispute's identity, stage and clock — the same on every screen. */
const DisputeHeader = ({ dispute, now, view, back, aside }: DisputeHeaderProps) => {
  const stage = stageOf(dispute, now)
  const deadline = stageDeadline(dispute, stage)
  const msLeft = deadline === null ? null : (deadline - now) * 1000

  return (
    <div className="mb-5 flex flex-wrap items-start justify-between gap-4 border-b border-border pb-4">
      <div className="min-w-0">
        <Link
          to={back.to}
          className="focus-ring label-xs inline-flex items-center gap-1 text-muted-foreground transition-colors hover:text-foreground"
        >
          <ChevronLeft className="h-3 w-3" /> {back.label}
        </Link>
        <div className="mt-2.5 flex flex-wrap items-center gap-x-3 gap-y-2">
          <h1 className="text-[15px] font-semibold tracking-tight text-foreground">Hearing</h1>
          <HashRef value={dispute.pda} lead={6} tail={6} href={explorerAddress(dispute.pda)} />
          <StageChip stage={stage} />
          <span className="label-xs text-unestablished">{view}</span>
        </div>
        <p className="mono tabular mt-2 text-[13px] text-foreground">
          {formatAmount(dispute.amount)}
          <span className="ml-1.5 text-[11px] text-muted-foreground">
            in dispute · opened {formatAgo(dispute.openedAt, now)}
            {dispute.escalated ? ' · escalated to a wider panel' : ''}
          </span>
        </p>
      </div>
      <div className="flex flex-col items-end gap-3">
        {msLeft === null ? null : (
          <Countdown
            msLeft={msLeft}
            urgent={msLeft < 15_000}
            size="lg"
            label={`${STAGE_LABEL[stage]} ends in`}
          />
        )}
        {aside}
      </div>
    </div>
  )
}

export default DisputeHeader
