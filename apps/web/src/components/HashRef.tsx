import { ExternalLink } from 'lucide-react'
import { useState } from 'react'
import { shortenMiddle } from '@/lib/dispute'

interface HashRefProps {
  value: string
  lead?: number
  tail?: number
  tone?: 'default' | 'muted'
  /** Where the value can be checked — the explorer page of the signature or account. */
  href?: string
}

/** Signatures and addresses are shortened in the middle and expand in place on click. */
const HashRef = ({ value, lead = 4, tail = 4, tone = 'default', href }: HashRefProps) => {
  const [open, setOpen] = useState(false)

  return (
    <span className="inline-flex max-w-full items-center gap-1">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        title={open ? 'Collapse' : 'Show in full'}
        className={`mono focus-ring inline-block max-w-full break-all rounded-sm border border-border bg-surface-2 px-1.5 py-0.5 text-left text-[11px] leading-[1.5] transition-colors hover:border-border-strong ${
          tone === 'muted' ? 'text-unestablished' : 'text-muted-foreground'
        }`}
      >
        {open ? value : shortenMiddle(value, lead, tail)}
      </button>
      {href ? (
        <a
          href={href}
          target="_blank"
          rel="noreferrer"
          title="Open in Solana Explorer"
          className="focus-ring shrink-0 rounded-sm p-0.5 text-unestablished transition-colors hover:text-foreground"
        >
          <ExternalLink className="h-3 w-3" />
          <span className="sr-only">Open in Solana Explorer</span>
        </a>
      ) : null}
    </span>
  )
}

export default HashRef
