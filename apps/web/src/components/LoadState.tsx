import { useEffect, useState } from 'react'
import { Link } from 'react-router-dom'
import { API_URL, ApiFailure } from '@/lib/api'

/** How long a read may take before the wait is explained, in milliseconds. */
const SLOW_MS = 5_000

/**
 * A loading line that does not shift the layout when the data lands. Past a
 * few seconds it says why: the api runs on a free host that sleeps when idle,
 * and its first answer after a sleep takes most of a minute (measured: 44 s).
 * A silent spinner for that long reads as a broken page.
 */
export const Loading = ({ what }: { what: string }) => {
  const [slow, setSlow] = useState(false)
  useEffect(() => {
    const timer = setTimeout(() => setSlow(true), SLOW_MS)
    return () => clearTimeout(timer)
  }, [])

  return (
    <div className="flex flex-col gap-1.5">
      <p className="label-xs animate-pulse text-unestablished">Reading {what}…</p>
      {slow ? (
        <p className="max-w-[560px] text-[12.5px] leading-relaxed text-muted-foreground">
          The dispute service runs on a free host that sleeps when nobody is using it. It is waking
          up now, which takes up to a minute; after that, pages open in under a second.
        </p>
      ) : null}
    </div>
  )
}

/**
 * What went wrong, said plainly. A sleeping free host takes up to a minute to
 * wake, and that is the most likely failure — so it is named, not hidden.
 */
export const Failure = ({ error }: { error: unknown }) => {
  const notFound = error instanceof ApiFailure && error.status === 404
  const unreachable = error instanceof ApiFailure && error.status === null

  if (API_URL === null) {
    return (
      <div className="rounded border border-border bg-surface px-4 py-3.5">
        <p className="text-[13.5px] font-medium text-foreground">
          This build is not connected to a dispute service.
        </p>
        <p className="mt-1.5 text-[12.5px] leading-relaxed text-muted-foreground">
          It was made without <span className="mono">VITE_API_URL</span>, so there is nothing to
          read hearings from.
        </p>
      </div>
    )
  }

  return (
    <div className="rounded border border-border bg-surface px-4 py-3.5">
      <p className="text-[13.5px] font-medium text-foreground">
        {notFound
          ? 'No such hearing.'
          : unreachable
            ? 'The dispute service is not answering.'
            : 'The dispute service answered with an error.'}
      </p>
      <p className="mt-1.5 text-[12.5px] leading-relaxed text-muted-foreground">
        {notFound ? (
          <>
            The address is not a dispute this service has seen.{' '}
            <Link to="/" className="text-foreground underline">
              Back to the hearings
            </Link>
          </>
        ) : unreachable ? (
          'It runs on a free host that sleeps when idle and can take up to a minute to wake. This page retries on its own.'
        ) : error instanceof Error ? (
          error.message
        ) : (
          'Unknown error.'
        )}
      </p>
    </div>
  )
}
