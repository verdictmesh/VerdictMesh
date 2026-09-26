import type { BorshCoder } from '@coral-xyz/anchor'

/**
 * Anchor events out of a transaction log, attributed to the program that
 * emitted them.
 *
 * **Not `EventParser` from `@coral-xyz/anchor`, and on purpose.** In 0.32 its
 * call stack records any cross-program invocation as the literal `"cpi"`
 * rather than the address being invoked, so an event emitted by a program
 * **called through CPI** is never attributed to it and never decoded. That is
 * exactly how disputes are opened: the escrow calls `open_dispute` through CPI,
 * and `DisputeOpened` with `DepositCollected` sit at depth 2 of every opening
 * transaction on devnet. The parser here keeps the real address of every frame.
 */

export interface LogEvent {
  /** The address of the program that emitted the event. */
  programId: string
  name: string
  data: Record<string, unknown>
}

export interface ParsedLogs {
  /** In order of emission, across all programs. */
  events: LogEvent[]
  /** Every program that ran, at any depth, in order of first call. */
  programs: string[]
  /**
   * The runtime cut the log at its 10 KB cap. Whatever was emitted after the
   * cut is gone for good, so the silence of this log proves nothing.
   */
  truncated: boolean
}

const INVOKE = /^Program ([1-9A-HJ-NP-Za-km-z]+) invoke \[(\d+)\]$/
const EXIT = /^Program ([1-9A-HJ-NP-Za-km-z]+) (?:success|failed)/
const DATA = 'Program data: '
const TRUNCATED = 'Log truncated'

/**
 * `coders` maps a program address **in the network** to the coder of its IDL.
 * An event is decoded only if the frame it was logged in belongs to a program
 * from that map: a foreign program is free to log bytes that start with our
 * discriminator, and it does not get to speak for us.
 */
export function parseLogs(
  logs: readonly string[],
  coders: ReadonlyMap<string, BorshCoder>,
): ParsedLogs {
  const stack: string[] = []
  const events: LogEvent[] = []
  const programs = new Set<string>()
  let truncated = false

  for (const line of logs) {
    if (line === TRUNCATED) {
      truncated = true
      break
    }

    const invoke = INVOKE.exec(line)
    if (invoke?.[1]) {
      stack.push(invoke[1])
      programs.add(invoke[1])
      continue
    }

    const exit = EXIT.exec(line)
    if (exit) {
      stack.pop()
      continue
    }

    if (!line.startsWith(DATA)) continue

    const programId = stack.at(-1)
    const coder = programId === undefined ? undefined : coders.get(programId)
    if (programId === undefined || !coder) continue

    // `decode` answers `null` to bytes that are not one of the program's
    // events: `sol_log_data` is open to any payload, not only to `emit!`.
    const event = coder.events.decode(line.slice(DATA.length))
    if (event) events.push({ programId, name: event.name, data: event.data })
  }

  return { events, programs: [...programs], truncated }
}
