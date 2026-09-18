import type { ReactNode } from 'react'
import { Link, useLocation } from 'react-router-dom'

interface ShellProps {
  children: ReactNode
}

const Shell = ({ children }: ShellProps) => {
  const { pathname } = useLocation()

  return (
    <div className="min-h-screen bg-background">
      <header className="sticky top-0 z-20 border-b border-border bg-background/95 backdrop-blur-[2px]">
        <div className="mx-auto flex h-12 max-w-[1180px] items-center justify-between px-5">
          <div className="flex items-baseline gap-4">
            <Link
              to="/"
              className="mono text-[13px] font-semibold tracking-[0.16em] text-foreground focus-ring"
            >
              VERDICTMESH
            </Link>
            <span className="label-xs hidden text-muted-foreground sm:inline">
              dispute resolution layer
            </span>
          </div>
          <nav className="flex items-center gap-1">
            <Link
              to="/"
              className={`label-xs rounded-sm px-2 py-1.5 focus-ring ${
                pathname === '/'
                  ? 'bg-surface-2 text-foreground'
                  : 'text-muted-foreground hover:text-foreground'
              }`}
            >
              Juror panel
            </Link>
            <Link
              to="/settlement/VM-1039"
              className={`label-xs rounded-sm px-2 py-1.5 focus-ring ${
                pathname.startsWith('/settlement')
                  ? 'bg-surface-2 text-foreground'
                  : 'text-muted-foreground hover:text-foreground'
              }`}
            >
              Settled
            </Link>
          </nav>
        </div>
      </header>
      <main className="mx-auto max-w-[1180px] px-5 pb-24 pt-6">{children}</main>
      <footer className="border-t border-border">
        <div className="mx-auto max-w-[1180px] px-5 py-4">
          <p className="label-xs text-unestablished">
            Demo interface · all deals, parties and signatures shown here are invented
          </p>
        </div>
      </footer>
    </div>
  )
}

export default Shell
