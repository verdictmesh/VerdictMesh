import { formatDuration } from '@/lib/verdictmesh'

interface CountdownProps {
  msLeft: number
  urgent?: boolean
  size?: 'md' | 'lg'
  label?: string
}

/**
 * The countdown is the loudest element on any screen it appears on:
 * a juror who misses the reveal window loses part of their stake.
 */
const Countdown = ({
  msLeft,
  urgent = false,
  size = 'md',
  label = 'Time left in window',
}: CountdownProps) => {
  const closed = msLeft <= 0

  return (
    <div className="flex flex-col items-end gap-1">
      <span className="label-xs text-muted-foreground">{closed ? 'Window closed' : label}</span>
      <span
        className={`mono tabular font-semibold leading-none ${
          size === 'lg' ? 'text-[40px]' : 'text-[26px]'
        } ${closed ? 'text-unestablished' : urgent ? 'text-clock-urgent' : 'text-clock'}`}
      >
        {closed ? '00s' : formatDuration(msLeft)}
      </span>
    </div>
  )
}

export default Countdown
