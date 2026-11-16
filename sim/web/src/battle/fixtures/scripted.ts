// Sources that play a scene, one per seat. They follow the scene's script and decide nothing about
// the rules: an answer has to be one the open question offers, as with any host, and whether the
// scene goes on from it, and to which table, is the script's to say. Each seat has its own source,
// as with the Worker's per-seat channels, and the two share one playing of the scene. A command is
// answered the way the Worker answers one: later, the receipt first, then what it changed.
import type {
  Answer,
  BattleUpdate,
  Command,
  HostMessage,
  OptionId,
  Question,
  Receipt,
  Seat,
  Selection,
} from "../contract"
import type { BattleSource } from "../source"
import {
  distribution,
  inRange,
  integer,
  ordering,
  partition,
  pileProblems,
  shareProblems,
  wholeOrder,
} from "../view/arrange"
import { designated, driveTrigger } from "../view/payments"
import { costTotal, problems } from "../view/select"
import { update } from "./build"
import type { Scene } from "./scenes"

const SEATS: readonly Seat[] = ["p1", "p2"]

/** The command's answer when the question offers it; null for anything the question does not list. */
function offered(question: Question | null, command: Command): Answer | null {
  if (question === null || command.kind !== "answer" || command.question_id !== question.id)
    return null
  const { answer } = command
  const { shape } = question
  if (answer.kind === "action_window" && shape.kind === "action_window") {
    const intent = shape.intents.find((listed) => listed.id === answer.intent)
    if (intent === undefined) return null
    const named =
      intent.kind === "attack"
        ? intent.targets.some((listed) => listed.id === answer.target)
        : answer.target === null
    return named ? answer : null
  }
  // Passing is always an answer to a Quick window.
  if (answer.kind === "quick" && shape.kind === "quick")
    return answer.option === null || shape.options.some((listed) => listed.id === answer.option)
      ? answer
      : null
  if (answer.kind === "select" && shape.kind === "select")
    return selected(shape.selection, answer) ? answer : null
  if (answer.kind === "choose" && shape.kind === "choose") {
    const { chosen } = answer
    const listed = chosen.every((id) => shape.options.some((option) => option.id === id))
    const within =
      chosen.length >= shape.bounds.min &&
      (shape.bounds.max === null || chosen.length <= shape.bounds.max)
    return listed && within && new Set(chosen).size === chosen.length ? answer : null
  }
  if (answer.kind === "mulligan" && shape.kind === "mulligan") {
    const order = answer.bottom_order
    // Keeping returns nothing; redrawing returns the whole hand, each card once.
    const whole =
      order.length === shape.hand.length &&
      shape.hand.every((card) => order.filter((id) => id === card.id).length === 1)
    return (answer.redraw ? whole : order.length === 0) ? answer : null
  }
  if (answer.kind === "confirm" && shape.kind === "confirm")
    return shape.allowed.includes(answer.accept) ? answer : null
  return compound(question, answer) ? answer : null
}

/**
 * Whether an answer to one of the arranging, amount, payment, roll or trigger questions stays
 * within what the question states, by the same readings the screen hints with.
 */
function compound(question: Question, answer: Answer): boolean {
  const { shape } = question
  const seat = question.actor
  if (answer.kind === "order" && shape.kind === "order") {
    const asked = ordering(question, seat)
    return asked !== null && wholeOrder(asked, answer.order)
  }
  if (answer.kind === "partition" && shape.kind === "partition") {
    const asked = partition(question, seat)
    if (asked === null) return false
    // Every pile of the question once, and no item on two of them.
    const piles = new Map(answer.buckets.map((given) => [given.bucket, given.items]))
    const items = answer.buckets.flatMap((given) => given.items)
    return (
      piles.size === answer.buckets.length &&
      answer.buckets.length === shape.buckets.length &&
      shape.buckets.every((listed) => piles.has(listed.bucket)) &&
      new Set(items).size === items.length &&
      items.every((id) => shape.items.some((item) => item.id === id)) &&
      pileProblems(asked, piles).length === 0
    )
  }
  if (answer.kind === "distribute" && shape.kind === "distribute") {
    const asked = distribution(question, seat)
    if (asked === null) return false
    // Every recipient once; the amounts, and whether an answer is taken at all with nobody to
    // deal to, are the reader's to say.
    const shares = new Map(answer.shares.map((share) => [share.option, share.amount]))
    return (
      shares.size === answer.shares.length &&
      answer.shares.length === shape.recipients.length &&
      shape.recipients.every((recipient) => shares.has(recipient.id)) &&
      shareProblems(asked, shares).length === 0
    )
  }
  if (answer.kind === "integer" && shape.kind === "integer") {
    const asked = integer(question, seat)
    return asked !== null && inRange(asked, answer.value)
  }
  if (answer.kind === "dice_reroll" && shape.kind === "dice_reroll")
    return shape.allowed.includes(answer.choice)
  if (answer.kind === "evolution_payment" && shape.kind === "evolution_payment")
    return shape.payments.some(
      (listed) =>
        listed.pp === answer.pp &&
        listed.use_ep === answer.use_ep &&
        listed.super_evolution === answer.super_evolution,
    )
  // A payment names its resources and nothing else, so they are held to the selection as picks
  // that are given no group and no role.
  if (answer.kind === "resource_payment" && shape.kind === "resource_payment")
    return (
      (shape.ep_allowed || !answer.use_ep) &&
      selected(shape.materials, {
        kind: "select",
        selected: answer.materials,
        groups: shape.materials.group_assignment === "explicit" ? [] : null,
        roles: [],
      })
    )
  if (answer.kind === "drive_trigger" && shape.kind === "drive_trigger") {
    const asked = driveTrigger(question, seat)
    if (asked === null || !shape.allowed.includes(answer.accept)) return false
    if (!answer.accept) return answer.target === null
    const listed =
      answer.target === null || shape.targets.some((target) => target.id === answer.target)
    return listed && designated(asked, answer.target)
  }
  return false
}

/**
 * Whether a selection's answer is one the question's own statement allows: its candidates, each
 * once, with every group and role the question lists, within the stated counts. Which candidates
 * the rules would allow is not asked: the question listed them.
 */
function selected(selection: Selection, answer: Extract<Answer, { kind: "select" }>): boolean {
  const picks = answer.selected
  if (new Set(picks).size !== picks.length) return false
  if (!picks.every((id) => selection.candidates.some((candidate) => candidate.id === id)))
    return false
  // Named groups come with every group of the question, each pick in exactly one of them.
  const explicit = selection.group_assignment === "explicit"
  if (explicit !== (answer.groups !== null)) return false
  const groups = new Map<OptionId, number>()
  for (const given of answer.groups ?? [])
    for (const id of given.selected) {
      if (groups.has(id) || !picks.includes(id)) return false
      groups.set(id, given.group)
    }
  // Each listed name once, and no other.
  const named = (listed: readonly number[], given: readonly number[]): boolean =>
    given.length === listed.length &&
    listed.every((name) => given.filter((other) => other === name).length === 1)
  if (
    explicit &&
    !named(
      selection.groups.map((each) => each.group),
      (answer.groups ?? []).map((each) => each.group),
    )
  )
    return false
  // The roles split the selection completely: every pick in exactly one.
  const roles = new Map<OptionId, number>()
  for (const given of answer.roles)
    for (const id of given.selected) {
      if (roles.has(id) || !picks.includes(id)) return false
      roles.set(id, given.role)
    }
  if (
    !named(
      selection.roles.map((each) => each.role),
      answer.roles.map((each) => each.role),
    )
  )
    return false
  if (selection.roles.length > 0 && roles.size !== picks.length) return false
  const asked = { question: "", selection }
  const made = { selected: picks, groups, roles }
  if (problems(asked, made).length > 0) return false
  const total = costTotal(asked, made)
  return (
    total.sum === null ||
    ((total.min === null || total.sum >= total.min) &&
      (total.max === null || total.sum <= total.max))
  )
}

/** What an update tells its seat, apart from its number. */
const bare = (message: BattleUpdate): string => JSON.stringify({ ...message, update_seq: 0 })

/** What a question asks, apart from the id it is asked under. */
const asks = (question: Question): string => JSON.stringify({ ...question, id: "" })

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
  /** The number of the last update each seat was sent, and that update without what only names it. */
  const told: Record<Seat, number> = { p1: 0, p2: 0 }
  const known: Record<Seat, string> = { p1: "", p2: "" }
  const applied: Answer[] = []

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
      if (message.kind === "update") {
        told[seat] = message.update_seq
        known[seat] = bare(message)
      }
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
          const chosen = offered(asked[seat], command)
          const step = chosen === null ? null : scene.after([...applied, chosen])
          if (chosen === null || step === null) {
            out[seat] = null
            resolve(receipt("rejected"))
            return
          }
          applied.push(chosen)
          const answered = asked[seat]?.id
          const updates = SEATS.flatMap((each) => {
            const number = told[each] + 1
            const built = update(step.table, each, number, step.input ?? `q${String(number)}`)
            const open = asked[each]
            let next = built
            if (each === seat) {
              // The answer was taken, so its question is over for good: whatever the seat is
              // asked next is another question under another id, even when it reads the same.
              if (built.input.kind === "question" && built.input.question.id === answered)
                throw new Error("the scene asks again under the id of a question already answered")
            } else {
              // An answer that was not its own ends no question of this seat: asked the same
              // thing still, the seat keeps the question it has, under the id it has.
              if (
                built.input.kind === "question" &&
                open !== null &&
                asks(built.input.question) === asks(open)
              )
                next = { ...built, input: { kind: "question", question: open } }
              // A step that shows the seat nothing new sends it nothing, not even a number: a
              // seat's sequence counts what that seat was told, and another seat's private
              // answer leaves no trace in it.
              if (bare(next) === known[each]) return []
            }
            told[each] = number
            known[each] = bare(next)
            return [[each, next] as const]
          })
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
