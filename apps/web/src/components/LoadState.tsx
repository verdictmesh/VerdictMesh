import { Link } from 'react-router-dom'
import { API_URL, ApiFailure } from '@/lib/api'

/** A loading line that does not shift the layout when the data lands. */
export const Loading = ({ what }: { what: string }) => (
  <p className="label-xs animate-pulse text-unestablished">Reading {what}…</p>
)

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
