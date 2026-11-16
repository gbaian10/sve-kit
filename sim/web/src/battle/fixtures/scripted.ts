// Sources that play a scene, one per seat. They follow the scene's script and decide nothing about
// the rules: an answer has to be one the open question offers, as with any host, and whether the
// scene goes on from it, and to which table, is the script's to say. Each seat has its own source,
// as with the Worker's per-seat channels, and the two share one playing of the scene.
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

/** Both seats' sources for one playing of a scene. */
export function scriptedTable(scene: Scene): Readonly<Record<Seat, BattleSource>> {
  const delivered: Record<Seat, HostMessage[]> = { p1: [], p2: [] }
  const listeners: Record<Seat, Set<(message: HostMessage) => void>> = {
    p1: new Set(),
    p2: new Set(),
  }
  const open: Record<Seat, Question | null> = { p1: null, p2: null }
  const closed: Record<Seat, boolean> = { p1: false, p2: false }
  const applied: Choice[] = []
  let requests = 0

  const deliver = (seat: Seat, message: HostMessage): void => {
    delivered[seat].push(message)
    if (message.kind === "update")
      open[seat] =
        message.input.kind === "question" && message.input.question.actor === seat
          ? message.input.question
          : null
    // Nothing may be answered once the host has stopped.
    if (message.kind === "host_error") open[seat] = null
    for (const listener of listeners[seat]) listener(message)
  }
  for (const seat of SEATS) for (const message of scene.script[seat]) deliver(seat, message)

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
      requests += 1
      const receipt = (result: Receipt["result"]): Receipt => ({
        kind: "receipt",
        request_id: `${seat}-c${String(requests)}`,
        result,
        rejection: result === "rejected" ? "invalid_submission" : null,
      })
      const chosen = choice(open[seat], command)
      const table = chosen === null ? null : scene.after([...applied, chosen])
      if (chosen === null || table === null) return Promise.resolve(receipt("rejected"))
      applied.push(chosen)
      const seq = applied.length
      for (const each of SEATS) deliver(each, update(table, each, seq, `q${String(seq)}`))
      return Promise.resolve(receipt("applied"))
    },
    close() {
      closed[seat] = true
      listeners[seat].clear()
    },
  })
  return { p1: source("p1"), p2: source("p2") }
}
