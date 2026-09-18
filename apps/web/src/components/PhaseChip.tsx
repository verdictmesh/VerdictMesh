import { PHASE_LABEL, type Phase } from '@/lib/verdictmesh'

interface PhaseChipProps {
  phase: Phase
}

const TONE: Record<Phase, string> = {
  commit: 'border-border-strong text-foreground',
  reveal: 'border-claimed/60 text-claimed',
  appeal: 'border-border text-muted-foreground',
  settled: 'border-confirmed/50 text-confirmed',
}

const PhaseChip = ({ phase }: PhaseChipProps) => (
  <span className={`label-xs rounded-sm border px-1.5 py-1 ${TONE[phase]}`}>
    {PHASE_LABEL[phase]}
  </span>
)

export default PhaseChip
