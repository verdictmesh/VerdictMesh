import { STAGE_LABEL, type Stage } from '@/lib/dispute'

const TONE: Record<Stage, string> = {
  optimistic: 'border-border text-muted-foreground',
  commit: 'border-border-strong text-foreground',
  reveal: 'border-claimed/60 text-claimed',
  tally: 'border-border text-muted-foreground',
  appeal: 'border-border text-muted-foreground',
  final: 'border-border-strong text-foreground',
  settled: 'border-confirmed/50 text-confirmed',
}

const StageChip = ({ stage }: { stage: Stage }) => (
  <span className={`label-xs rounded-sm border px-1.5 py-1 ${TONE[stage]}`}>
    {STAGE_LABEL[stage]}
  </span>
)

export default StageChip
