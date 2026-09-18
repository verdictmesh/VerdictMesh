import { useState } from 'react'
import { shortenMiddle } from '@/lib/verdictmesh'

interface HashRefProps {
  value: string
  lead?: number
  tail?: number
  tone?: 'default' | 'muted'
}

/** Signatures and addresses are shortened in the middle and expand in place on click. */
const HashRef = ({ value, lead = 4, tail = 4, tone = 'default' }: HashRefProps) => {
  const [open, setOpen] = useState(false)

  return (
    <button
      type="button"
      onClick={() => setOpen((v) => !v)}
      title={open ? 'Collapse' : 'Show full signature'}
      className={`mono focus-ring inline-block max-w-full break-all rounded-sm border border-border bg-surface-2 px-1.5 py-0.5 text-[11px] leading-[1.5] transition-colors hover:border-border-strong ${
        tone === 'muted' ? 'text-unestablished' : 'text-muted-foreground'
      }`}
    >
      {open ? value : shortenMiddle(value, lead, tail)}
    </button>
  )
}

export default HashRef
