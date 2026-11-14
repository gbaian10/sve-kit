// @vitest-environment node
import { describe, expect, it } from "vitest"

import type {
  BattleEvent,
  BattleInput,
  BattleObservation,
  BattleUpdate,
  EventBatch,
  PresentationOp,
  Question,
} from "../contract"
import { accept, advance, type Beat, EMPTY, type PresentationState, skip } from "./queue"

// Only identity matters here: the queue never looks inside a board, a question or an event body.
const board = (name: string) => ({ name }) as unknown as BattleObservation
const question = (id: string) => ({ id }) as unknown as Question
const body = (name: string) => ({ kind: name }) as unknown as BattleEvent["body"]
const about = (name: string) => [{ kind: name }] as unknown as BattleEvent["about"]
const waiting = { kind: "waiting", actor: "p2", context: {} } as unknown as BattleInput
const fault: BattleInput = { kind: "engine_fault", can_concede: true }
const ask = (id: string): BattleInput => ({ kind: "question", question: question(id) })

/** A committed batch; `named` is the batch name its bodies carry when it differs from the id. */
function committed(
  id: string,
  links: (string | null)[] = [null],
  named = id,
  subjects: BattleEvent["about"] = [],
): EventBatch {
  return {
    id,
    events: links.map((presentation_id, index) => ({
      id: `${id}-e${String(index)}`,
      cause: "c",
      parent: null,
      body: body(`${named}-${String(index)}`),
      about: subjects,
      presentation_id,
    })),
  }
}

/** A scope whose events are named after the committed batch that will promote it. */
function provisional(scope: string, promotedBy: string, members = 1): PresentationOp {
  return {
    kind: "provisional",
    scope_id: scope,
    batch: {
      id: `${scope}-batch`,
      events: Array.from({ length: members }, (_, index) => ({
        id: `${scope}-p${String(index)}`,
        body: body(`${promotedBy}-${String(index)}`),
        about: [],
      })),
    },
  }
}

const promote = (scope: string): PresentationOp => ({ kind: "promote", scope_id: scope })
const cancel = (scope: string): PresentationOp => ({
  kind: "cancel",
  scope_id: scope,
  reason: "engine_fault",
})

let sequence = 0
function update(parts: {
  board?: string
  input?: BattleInput
  batches?: EventBatch[]
  animate?: string[]
  presentation?: PresentationOp[]
  unavailable?: boolean
}): BattleUpdate {
  sequence += 1
  return {
    kind: "update",
    format: 1,
    viewer: "p1",
    update_seq: sequence,
    observation: board(parts.board ?? `board-${String(sequence)}`),
    input: parts.input ?? waiting,
    events:
      parts.unavailable === true
        ? { status: "unavailable", reason: "projection_gap" }
        : {
            status: "available",
            batches: parts.batches ?? [],
            animate: parts.animate ?? (parts.batches ?? []).map((batch) => batch.id),
          },
    presentation: parts.presentation ?? [],
    notices: [],
    advice: [],
  }
}

function accepted(state: PresentationState, next: BattleUpdate): PresentationState {
  const result = accept(state, next)
  if ("kind" in result) throw new Error("update rejected")
  return result
}

/** Play everything queued and return the beats in the order they were handed out. */
function play(state: PresentationState): { state: PresentationState; beats: string[] } {
  const beats: string[] = []
  let current = state
  for (;;) {
    const step = advance(current)
    current = step.state
    if (step.beat === null) return { state: current, beats }
    beats.push(name(step.beat))
  }
}

const name = (beat: Beat) => (beat.kind === "committed" ? beat.batch.id : beat.scope_id)

describe("presentation queue", () => {
  it("settles a baseline without playing anything and enables its question", () => {
    const { state, beats } = play(accepted(EMPTY, update({ board: "start", input: ask("q1") })))
    expect(beats).toEqual([])
    expect(state.board).toEqual(board("start"))
    expect(state.question).toEqual(question("q1"))
  })

  it("plays committed batches in batch order, then provisional ones in delivery order", () => {
    const start = play(accepted(EMPTY, update({ board: "start" }))).state
    const queued = accepted(
      start,
      update({
        board: "after",
        input: ask("q2"),
        batches: [committed("b1"), committed("b2")],
        animate: ["b2", "b1"],
        presentation: [provisional("s1", "x"), provisional("s2", "y")],
      }),
    )
    expect(queued.question).toBeNull()

    const first = advance(queued)
    expect(first.beat && name(first.beat)).toBe("b1")
    expect(first.state.board).toEqual(board("start"))
    expect(first.state.question).toBeNull()

    const rest = play(first.state)
    expect(rest.beats).toEqual(["b2", "s1", "s2"])
    expect(rest.state.board).toEqual(board("after"))
    expect(rest.state.question).toEqual(question("q2"))
  })

  it("does not play a committed batch that is absent from animate", () => {
    const state = accepted(
      EMPTY,
      update({ batches: [committed("old"), committed("new")], animate: ["new"] }),
    )
    expect(play(state).beats).toEqual(["new"])
  })

  it("never plays a promoted scope again, even when the host lists its batch", () => {
    const shown = play(accepted(EMPTY, update({ presentation: [provisional("s1", "b1")] })))
    expect(shown.beats).toEqual(["s1"])
    const promoted = accepted(
      shown.state,
      update({
        batches: [committed("b1", ["s1-p0"]), committed("b2")],
        animate: ["b1", "b2"],
        presentation: [promote("s1")],
      }),
    )
    expect(play(promoted).beats).toEqual(["b2"])
    expect(promoted.scopes.get("s1")?.life).toBe("promoted")
  })

  it("still plays a queued scope that is promoted before its turn, and not its committed batch", () => {
    const queued = accepted(
      EMPTY,
      update({ board: "one", batches: [committed("a")], presentation: [provisional("s1", "c")] }),
    )
    const playing = advance(queued)
    expect(playing.beat && name(playing.beat)).toBe("a")
    // The opponent kept resolving while this seat was still watching the first batch.
    const promoted = accepted(
      playing.state,
      update({
        board: "two",
        input: ask("q"),
        batches: [committed("c", ["s1-p0"]), committed("d")],
        presentation: [promote("s1")],
      }),
    )
    const rest = play(promoted)
    expect(rest.beats).toEqual(["s1", "d"])
    expect(rest.state.board).toEqual(board("two"))
    expect(rest.state.question).toEqual(question("q"))
  })

  it("rejects a promotion whose committed subjects differ from what the scope showed", () => {
    const shown = accepted(EMPTY, update({ presentation: [provisional("s1", "b1")] }))
    const changed = accept(
      shown,
      update({
        batches: [committed("b1", ["s1-p0"], "b1", about("other"))],
        presentation: [promote("s1")],
      }),
    )
    expect(changed).toEqual({ kind: "host_error", reason: "projection" })
  })

  it("rejects committed batches that share, split or orphan a scope's occurrences", () => {
    const shown = accepted(EMPTY, update({ presentation: [provisional("s1", "b1", 2)] }))
    const twice = accept(
      shown,
      update({
        batches: [committed("b1", ["s1-p0", "s1-p1"]), committed("copy", ["s1-p0", "s1-p1"], "b1")],
        presentation: [promote("s1")],
      }),
    )
    const mixed = accept(
      shown,
      update({ batches: [committed("b1", ["s1-p0", null])], presentation: [promote("s1")] }),
    )
    const unannounced = accept(shown, update({ batches: [committed("b1", ["s1-p0", "s1-p1"])] }))
    // Even for a scope this screen never held, a batch is linked as a whole or not at all.
    const partly = accept(
      EMPTY,
      update({ batches: [committed("b1", ["gone-p0", null])], presentation: [promote("gone")] }),
    )
    for (const result of [twice, mixed, unannounced, partly])
      expect(result).toEqual({ kind: "host_error", reason: "projection" })
  })

  it("rejects a promotion whose committed batch is missing or differs from the scope", () => {
    const shown = accepted(EMPTY, update({ presentation: [provisional("s1", "b1", 2)] }))
    const missing = accept(shown, update({ presentation: [promote("s1")] }))
    const partial = accept(
      shown,
      update({ batches: [committed("b1", ["s1-p0"])], presentation: [promote("s1")] }),
    )
    const changed = accept(
      shown,
      update({
        batches: [committed("other", ["s1-p0", "s1-p1"])],
        presentation: [promote("s1")],
      }),
    )
    for (const result of [missing, partial, changed])
      expect(result).toEqual({ kind: "host_error", reason: "projection" })
  })

  it("accepts the promotion of a scope a fresh screen never held, as history only", () => {
    const state = accepted(
      EMPTY,
      update({ batches: [committed("b1", ["gone-p0"])], presentation: [promote("gone")] }),
    )
    expect(play(state).beats).toEqual([])
  })

  it("plays a repeated scope once and rejects the same scope with other content", () => {
    const shown = accepted(EMPTY, update({ presentation: [provisional("s1", "b1")] }))
    const repeated = accepted(shown, update({ presentation: [provisional("s1", "b1")] }))
    expect(play(repeated).beats).toEqual(["s1"])
    expect(accept(shown, update({ presentation: [provisional("s1", "other")] }))).toEqual({
      kind: "host_error",
      reason: "projection",
    })
  })

  it("rejects a second scope that reuses an occurrence, so no two scopes share a committed batch", () => {
    // Two scope ids whose events carry the same presentation ids, bodies and subjects.
    const first = provisional("s1", "c")
    const twin: PresentationOp = { ...first, scope_id: "s2" }
    const together = accept(EMPTY, update({ presentation: [first, twin] }))
    const later = accept(
      accepted(EMPTY, update({ presentation: [first] })),
      update({ presentation: [twin] }),
    )
    const closed = accept(
      accepted(
        accepted(EMPTY, update({ presentation: [first] })),
        update({ batches: [committed("c", ["s1-p0"])], presentation: [promote("s1")] }),
      ),
      update({ presentation: [twin] }),
    )
    const doubled = provisional("s3", "d", 2)
    if (doubled.kind === "provisional") {
      const [member] = doubled.batch.events
      if (member !== undefined) doubled.batch.events = [member, member]
    }
    const within = accept(EMPTY, update({ presentation: [doubled] }))
    for (const result of [together, later, closed, within])
      expect(result).toEqual({ kind: "host_error", reason: "projection" })
  })

  it("keeps a closed scope's content: a repeat is not replayed and a reuse is rejected", () => {
    const shown = play(
      accepted(EMPTY, update({ presentation: [provisional("s1", "b1"), provisional("s2", "x")] })),
    ).state
    const closed = accepted(
      shown,
      update({
        batches: [committed("b1", ["s1-p0"])],
        presentation: [promote("s1"), cancel("s2")],
      }),
    )
    for (const scope of [provisional("s1", "b1"), provisional("s2", "x")])
      expect(play(accepted(closed, update({ presentation: [scope] }))).beats).toEqual([])
    for (const scope of [provisional("s1", "new"), provisional("s2", "new")])
      expect(accept(closed, update({ presentation: [scope] }))).toEqual({
        kind: "host_error",
        reason: "projection",
      })
    for (const end of [promote("s1"), cancel("s1"), promote("s2"), cancel("s2")])
      expect(accept(closed, update({ presentation: [end] }))).toEqual({
        kind: "host_error",
        reason: "projection",
      })
  })

  it("does not replay a skipped scope that is delivered again", () => {
    const skipped = skip(accepted(EMPTY, update({ presentation: [provisional("s1", "b1")] })))
    const again = accepted(skipped, update({ presentation: [provisional("s1", "b1")] }))
    expect(play(again).beats).toEqual([])
  })

  it("drops a cancelled scope that has not played and keeps the rest in order", () => {
    const queued = accepted(
      EMPTY,
      update({ presentation: [provisional("s1", "x"), provisional("s2", "y")] }),
    )
    const first = advance(queued)
    expect(first.beat && name(first.beat)).toBe("s1")
    const cancelled = accepted(
      first.state,
      update({ board: "safe", batches: [committed("b9")], presentation: [cancel("s2")] }),
    )
    const rest = play(cancelled)
    expect(rest.beats).toEqual(["b9"])
    expect(rest.state.board).toEqual(board("safe"))
    expect(rest.state.scopes.get("s2")?.life).toBe("cancelled")
  })

  it("stops waiting for playback on an engine fault and aligns with the fault board", () => {
    const queued = accepted(
      EMPTY,
      update({ batches: [committed("b1")], presentation: [provisional("s1", "x")] }),
    )
    const faulted = accepted(
      queued,
      update({ board: "fault", input: fault, presentation: [cancel("s1")] }),
    )
    const { state, beats } = play(faulted)
    expect(beats).toEqual([])
    expect(state.board).toEqual(board("fault"))
    expect(state.question).toBeNull()

    // The seat concedes: the result is committed and listed for animation, but a faulted
    // match only follows boards from now on.
    const finished: BattleInput = { kind: "finished", winner: "p2", reason: "concession" }
    const conceded = play(
      accepted(
        state,
        update({
          board: "result",
          input: finished,
          batches: [committed("result")],
          presentation: [provisional("late", "x")],
        }),
      ),
    )
    expect(conceded.beats).toEqual([])
    expect(conceded.state.board).toEqual(board("result"))
  })

  it("goes silent when events become unavailable but keeps following the board", () => {
    const queued = accepted(EMPTY, update({ presentation: [provisional("s1", "x")] }))
    const silent = accepted(queued, update({ board: "gap", unavailable: true, input: ask("q") }))
    const settled = play(silent)
    expect(settled.beats).toEqual([])
    expect(settled.state.board).toEqual(board("gap"))
    expect(settled.state.question).toEqual(question("q"))

    const later = play(accepted(settled.state, update({ board: "later", unavailable: true })))
    expect(later.beats).toEqual([])
    expect(later.state.board).toEqual(board("later"))
  })

  it("skips to the newest board and does not restart a skipped scope when it is promoted", () => {
    const queued = accepted(
      accepted(EMPTY, update({ board: "one", presentation: [provisional("s1", "b1")] })),
      update({ board: "two", input: ask("q2"), batches: [committed("b0")] }),
    )
    const skipped = skip(queued)
    expect(skipped.board).toEqual(board("two"))
    expect(skipped.question).toEqual(question("q2"))
    const promoted = accepted(
      skipped,
      update({ batches: [committed("b1", ["s1-p0"])], presentation: [promote("s1")] }),
    )
    expect(play(promoted).beats).toEqual([])
  })

  it("never replays a committed batch that an earlier update already delivered", () => {
    const first = play(accepted(EMPTY, update({ batches: [committed("b1")] })))
    expect(first.beats).toEqual(["b1"])
    const again = accepted(first.state, update({ batches: [committed("b1"), committed("b2")] }))
    expect(play(again).beats).toEqual(["b2"])
  })

  it("enables only the newest update's question, after everything before it settled", () => {
    const queued = accepted(
      accepted(EMPTY, update({ board: "one", input: ask("stale"), batches: [committed("b1")] })),
      update({ board: "two", input: ask("live"), batches: [committed("b2")] }),
    )
    const first = advance(queued)
    const second = advance(first.state)
    expect(second.beat && name(second.beat)).toBe("b2")
    expect(second.state.board).toEqual(board("one"))
    expect(second.state.question).toBeNull()
    const done = advance(second.state)
    expect(done.beat).toBeNull()
    expect(done.state.board).toEqual(board("two"))
    expect(done.state.question).toEqual(question("live"))
  })

  it("rejects an animate entry that names no batch of the same update", () => {
    expect(accept(EMPTY, update({ batches: [committed("b1")], animate: ["missing"] }))).toEqual({
      kind: "host_error",
      reason: "projection",
    })
  })
})
