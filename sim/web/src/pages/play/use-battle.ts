import { useEffect, useState } from "react"

import type {
  BattleInput,
  BattleObservation,
  HostError,
  HostMessage,
  Notice,
  Question,
  Ready,
} from "../../battle/contract"
import { NoticeInbox } from "../../battle/presentation/notices"
import { accept, EMPTY, type PresentationState, skip } from "../../battle/presentation/queue"
import type { BattleSource } from "../../battle/source"
import { place, type Placement, type SlotArrangement, UNPLACED } from "../../battle/view/table"

export interface BattleState {
  /** The match's identity; absent until the host says it is ready. */
  readonly ready: Ready | null
  /** The table on screen: the last update that settled. */
  readonly board: BattleObservation | null
  /** Where this screen has laid the board's field and EX cards. */
  readonly placement: Placement
  /** The question that may be answered now; null while none is open to this seat. */
  readonly question: Question | null
  /**
   * What the settled update says happens next: a question, waiting for a seat, the end of the
   * game, or an engine fault. Null before the first update.
   */
  readonly input: BattleInput | null
  /** Interaction stopped for good; nothing shown may be answered. */
  readonly stopped: HostError | null
  readonly notices: readonly Notice[]
  readonly dismissNotice: (id: string) => void
}

interface Progress {
  readonly ready: Ready | null
  readonly stopped: HostError | null
  readonly queue: PresentationState
  readonly placement: Placement
  readonly input: BattleInput | null
}

const START: Progress = {
  ready: null,
  stopped: null,
  queue: EMPTY,
  placement: UNPLACED,
  input: null,
}

function step(progress: Progress, message: HostMessage, first: SlotArrangement): Progress {
  if (progress.stopped !== null) return progress
  if (message.kind === "ready") return { ...progress, ready: message }
  if (message.kind === "host_error") return { ...progress, stopped: message }
  if (message.kind !== "update") return progress
  const accepted = accept(progress.queue, message)
  if ("kind" in accepted)
    return { ...progress, stopped: { kind: "host_error", format: 1, reason: accepted.reason } }
  // Nothing is animated yet: every update settles at once on its complete board.
  const queue = skip(accepted)
  return {
    ...progress,
    queue,
    input: message.input,
    placement:
      queue.board === null
        ? progress.placement
        : place(progress.placement, queue.board, progress.queue.board === null ? first : {}),
  }
}

const NOTHING_LAID: SlotArrangement = {}
const inboxes = new WeakMap<BattleSource, NoticeInbox>()

/**
 * One seat's screen state, fed by its source through the presentation queue. `first` lays the
 * cards of a table that starts with some on it.
 */
export function useBattle(
  source: BattleSource,
  first: SlotArrangement = NOTHING_LAID,
): BattleState {
  const [screen, setScreen] = useState<{
    source: BattleSource
    progress: Progress
    notices: readonly Notice[]
  }>()
  useEffect(() => {
    // A source replays its messages to a new listener, so each subscription starts over.
    let current = START
    const inbox = inboxes.get(source) ?? new NoticeInbox()
    inboxes.set(source, inbox)
    return source.listen((message) => {
      if (message.kind === "update") {
        if (message.viewer !== source.seat) return
        inbox.receive(message.notices)
      }
      current = step(current, message, first)
      setScreen({ source, progress: current, notices: inbox.pending() })
    })
  }, [source, first])
  const shown = screen?.source === source ? screen : undefined
  const progress = shown?.progress ?? START
  return {
    ready: progress.ready,
    board: progress.queue.board,
    placement: progress.placement,
    question: progress.queue.question,
    input: progress.input,
    stopped: progress.stopped,
    notices: shown?.notices ?? [],
    dismissNotice(id) {
      const inbox = inboxes.get(source)
      if (inbox === undefined) return
      inbox.dismiss(id)
      setScreen((current) =>
        current?.source === source ? { ...current, notices: inbox.pending() } : current,
      )
    },
  }
}
