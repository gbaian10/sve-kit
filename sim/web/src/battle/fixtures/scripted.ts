// Sources that play a scene, one per seat. They follow the scene's script and decide nothing about
// the rules: an answer has to be one the open question offers, as with any host, and whether the
// scene goes on from it, and to which table, is the script's to say. Each seat has its own source,
// as with the Worker's per-seat channels, and the two share one playing of the scene. A command is
// answered the way the Worker answers one: later, the receipt first, then what it changed.
import type { Command, HostMessage, Question, Receipt, Seat } from "../contract"
import type { BattleSource } from "../source"
import { update } from "./build"
import type { Choice, Scene } from "./scenes"

const SEATS: readonly Seat[] = ["p1", "p2"]

/** The answer as one of the question's own options; null when the question does not offer it. */
function choice(question: Question | null, command: Command): Choice | null {
  if (question === null || command.kind !== "answer" || command.question_id !== question.id)
    return null
  const { answer } = command
  if (answer.kind !== "action_window" || question.shape.kind !== "action_window") return null
  const intent = question.shape.intents.find((offered) => offered.id === answer.intent)
  if (intent === undefined) return null
  const named =
    intent.kind === "attack"
      ? intent.targets.some((offered) => offered.id === answer.target)
      : answer.target === null
  return named ? { intent: answer.intent, target: answer.target } : null
}

/** A later task, as each message from a Worker arrives in a task of its own. */
const later = (run: () => void): void => {
  setTimeout(run, 0)
}

/** Both seats' sources for one playing of a scene. */
export function scriptedTable(scene: Scene): Readonly<Record<Seat, BattleSource>> {
  const delivered: Record<Seat, HostMessage[]> = { p1: [], p2: [] }
  const listeners: Record<Seat, Set<(message: HostMessage) => void>> = {
    p1: new Set(),
    p2: new Set(),
  }
  /** What the host's latest update asks each seat, whether or not the update has arrived yet. */
  const asked: Record<Seat, Question | null> = { p1: null, p2: null }
  const closed: Record<Seat, boolean> = { p1: false, p2: false }
  const stopped: Record<Seat, boolean> = { p1: false, p2: false }
  /**
   * How to fail each seat's command that is still out. A command is out from `submit` until its
   * receipt is in and everything it changed has reached both seats: a seat sends one command at a
   * time, so nothing a listener sends while it is being told can overtake an update on its way.
   */
  const out: Record<Seat, ((reason: Error) => void) | null> = { p1: null, p2: null }
  const sent: Record<Seat, number> = { p1: 0, p2: 0 }
  const applied: Choice[] = []

  const produce = (seat: Seat, message: HostMessage): void => {
    if (message.kind === "update")
      asked[seat] =
        message.input.kind === "question" && message.input.question.actor === seat
          ? message.input.question
          : null
    // A host error ends the interaction for good: nothing is asked and nothing is taken.
    if (message.kind === "host_error") {
      asked[seat] = null
      stopped[seat] = true
    }
  }
  const deliver = (seat: Seat, message: HostMessage): void => {
    delivered[seat].push(message)
    // A listener that subscribes while this one is told gets the message from its replay.
    for (const listener of [...listeners[seat]])
      if (listeners[seat].has(listener)) listener(message)
  }
  for (const seat of SEATS)
    for (const message of scene.script[seat]) {
      produce(seat, message)
      deliver(seat, message)
    }

  const source = (seat: Seat): BattleSource => ({
    seat,
    listen(listener) {
      if (closed[seat]) return () => undefined
      for (const message of delivered[seat]) listener(message)
      listeners[seat].add(listener)
      return () => {
        listeners[seat].delete(listener)
      }
    },
    submit(command) {
      if (closed[seat]) return Promise.reject(new Error("the source is closed"))
      if (stopped[seat]) return Promise.reject(new Error("the host has stopped"))
      if (out[seat] !== null) return Promise.reject(new Error("another command is still out"))
      sent[seat] += 1
      const request_id = `${seat}-c${String(sent[seat])}`
      const receipt = (result: Receipt["result"]): Receipt => ({
        kind: "receipt",
        request_id,
        result,
        rejection: result === "rejected" ? "invalid_submission" : null,
      })
      return new Promise<Receipt>((resolve, reject) => {
        out[seat] = reject
        later(() => {
          // Closed in the meantime: `close` has failed the command already.
          if (out[seat] !== reject) return
          const chosen = choice(asked[seat], command)
          const table = chosen === null ? null : scene.after([...applied, chosen])
          if (chosen === null || table === null) {
            out[seat] = null
            resolve(receipt("rejected"))
            return
          }
          applied.push(chosen)
          const seq = applied.length
          const updates = SEATS.map(
            (each) => [each, update(table, each, seq, `q${String(seq)}`)] as const,
          )
          for (const [each, message] of updates) produce(each, message)
          resolve(receipt("applied"))
          // The receipt comes before the update it caused: the updates go out in a later task,
          // once everything the receipt set off has run.
          later(() => {
            for (const [each, message] of updates) deliver(each, message)
            if (out[seat] === reject) out[seat] = null
          })
        })
      })
    },
    close() {
      closed[seat] = true
      listeners[seat].clear()
      out[seat]?.(new Error("the source is closed"))
      out[seat] = null
    },
  })
  return { p1: source("p1"), p2: source("p2") }
}
