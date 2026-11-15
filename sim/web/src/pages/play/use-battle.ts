import { useEffect, useState } from "react"

import type { BattleObservation, HostError, HostMessage, Ready } from "../../battle/contract"
import { accept, EMPTY, type PresentationState, skip } from "../../battle/presentation/queue"
import type { BattleSource } from "../../battle/source"

export interface BattleState {
  /** The match's identity; absent until the host says it is ready. */
  readonly ready: Ready | null
  /** The table on screen: the last update that settled. */
  readonly board: BattleObservation | null
  /** Interaction stopped for good; nothing shown may be answered. */
  readonly stopped: HostError | null
}

interface Progress {
  readonly ready: Ready | null
  readonly stopped: HostError | null
  readonly queue: PresentationState
}

const START: Progress = { ready: null, stopped: null, queue: EMPTY }

function step(progress: Progress, message: HostMessage): Progress {
  if (progress.stopped !== null) return progress
  if (message.kind === "ready") return { ...progress, ready: message }
  if (message.kind === "host_error") return { ...progress, stopped: message }
  if (message.kind !== "update") return progress
  const accepted = accept(progress.queue, message)
  if ("kind" in accepted)
    return { ...progress, stopped: { kind: "host_error", format: 1, reason: accepted.reason } }
  // Nothing is animated yet: every update settles at once on its complete board.
  return { ...progress, queue: skip(accepted) }
}

/** One seat's screen state, fed by its source through the presentation queue. */
export function useBattle(source: BattleSource): BattleState {
  const [progress, setProgress] = useState(START)
  useEffect(() => {
    // A source replays its messages to a new listener, so each subscription starts over.
    let current = START
    return source.listen((message) => {
      current = step(current, message)
      setProgress(current)
    })
  }, [source])
  return { ready: progress.ready, board: progress.queue.board, stopped: progress.stopped }
}
