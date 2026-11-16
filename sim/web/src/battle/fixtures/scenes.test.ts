// @vitest-environment node
import { describe, expect, it } from "vitest"

import {
  type Answer,
  type BattleObservation,
  type BattleUpdate,
  type HostMessage,
  type Seat,
  validateHostMessage,
  type ZoneView,
} from "../contract"
import { update } from "./build"
import { SCENES } from "./scenes"

const SEATS: readonly Seat[] = ["p1", "p2"]

function baselineOf(messages: readonly HostMessage[]): BattleObservation {
  const update = messages.find((message): message is BattleUpdate => message.kind === "update")
  if (update === undefined) throw new Error("the script has no update")
  return update.observation
}

describe("fixture scenes", () => {
  it.each(SCENES)("$id sends each seat valid contract messages for that seat", ({ script }) => {
    for (const seat of SEATS) {
      expect(script[seat].length).toBeGreaterThan(1)
      for (const message of script[seat]) {
        expect(validateHostMessage(message)).toBe(true)
        // A host error names no seat; everything else is addressed to the one that receives it.
        if ("viewer" in message) expect(message.viewer).toBe(seat)
      }
    }
  })

  it.each(SCENES)("$id tells a seat nothing private about the other seat", ({ script }) => {
    for (const viewer of SEATS) {
      const observation = baselineOf(script[viewer])
      const other = observation.players.find((player) => player.seat !== viewer)
      if (other === undefined) throw new Error("the other seat is missing")
      // The other hand and the face-down part of the other evolve deck are sizes only.
      expect(other.zones.hand.cards).toEqual([])
      expect(other.zones.deck.cards).toEqual([])
      for (const view of Object.values(observation.objects)) {
        if (view.knowledge === "private") expect(view.owner).toBe(viewer)
        if (view.owner !== viewer) expect(view.facing).toBe("up")
      }
    }
  })

  it.each(SCENES)("$id gives both seats the same public table", ({ script }) => {
    const [first, second] = SEATS.map((seat) => baselineOf(script[seat]))
    if (first === undefined || second === undefined) throw new Error("a seat is missing")
    expect(first.turn).toEqual(second.turn)
    first.players.forEach((player, index) => {
      const mirror = second.players[index]
      expect(mirror?.zones.hand.count).toBe(player.zones.hand.count)
      expect(mirror?.zones.field.count).toBe(player.zones.field.count)
      expect(mirror?.zones.evolve_deck.count).toBe(player.zones.evolve_deck.count)
      expect(mirror?.life).toBe(player.life)
    })
    // A handle means something to one seat only.
    const shared = Object.keys(first.objects).filter((handle) => handle in second.objects)
    expect(shared).toEqual([])
  })

  it.each(SCENES)("$id gives every listed member a place inside its zone, once", ({ script }) => {
    // The schema allows any number; the authority only ever sends an offset below the zone's size.
    for (const seat of SEATS) {
      for (const player of baselineOf(script[seat]).players) {
        const zones: Record<string, ZoneView> = { ...player.zones }
        for (const zone of Object.values(zones)) {
          const places = zone.cards.flatMap(({ position }) => (position === null ? [] : [position]))
          expect(Math.max(-1, ...places)).toBeLessThan(zone.count)
          expect(new Set(places).size).toBe(places.length)
        }
      }
    }
  })

  it.each(SCENES)("$id keeps each seat's evolve points consistent", ({ script }) => {
    // The schema cannot say this either; the authority refuses to send a board that breaks it.
    for (const seat of SEATS) {
      for (const player of baselineOf(script[seat]).players) {
        expect(player.ep + player.ep_used).toBe(player.ep_capacity)
        if (player.sep > 0) expect(player.sep_available_from).not.toBeNull()
      }
    }
  })

  it.each(SCENES)("$id arranges a slot for each field card", ({ script, slots }) => {
    const [first, second] = baselineOf(script.p1).players
    expect(slots.p1?.field).toHaveLength(first.zones.field.count)
    expect(slots.p2?.field).toHaveLength(second.zones.field.count)
  })

  it.each(SCENES)("$id asks a seat only what is put to it", ({ script }) => {
    for (const seat of SEATS) {
      const first = script[seat].find((message) => message.kind === "update")
      if (first === undefined) throw new Error("the script has no update")
      const { turn } = first.observation
      if (first.input.kind === "question") {
        const { question } = first.input
        expect(question.actor).toBe(seat)
        // What to do in a main phase is asked of the seat whose main phase it is.
        if (question.shape.kind === "action_window")
          expect(turn).toMatchObject({ active: seat, phase: "main" })
      }
      // The seat that waits is told for whom, and nothing of what that seat is asked.
      if (first.input.kind === "waiting") {
        const other = script[seat === "p1" ? "p2" : "p1"].find(
          (message) => message.kind === "update",
        )
        expect(first.input.actor).toBe(
          other?.input.kind === "question" ? other.input.question.actor : (turn.active ?? "p1"),
        )
      }
    }
  })

  it("sends valid messages along every course of the played turn", () => {
    const turn = SCENES.find((entry) => entry.id === "turn")
    if (turn === undefined) throw new Error("no played turn")
    const courses: Answer[][] = [[]]
    let ended = 0
    for (let at = 0; at < courses.length; at += 1) {
      const choices = courses[at] ?? []
      const step = turn.after(choices)
      if (step === null) throw new Error("an offered answer led nowhere")
      const [own, other] = SEATS.map((seat) => update(step.table, seat, choices.length, "q"))
      if (own === undefined || other === undefined) throw new Error("a seat is missing")
      expect(validateHostMessage(own)).toBe(true)
      expect(validateHostMessage(other)).toBe(true)
      if (own.input.kind !== "question" || own.input.question.shape.kind !== "action_window") {
        // The turn is over: the first seat waits and the script takes nothing more.
        expect(own.input).toMatchObject({ kind: "waiting", actor: "p2" })
        expect(
          turn.after([...choices, { kind: "action_window", intent: "end", target: null }]),
        ).toBeNull()
        ended += 1
        continue
      }
      // Every option the question offers leads somewhere, so nothing offered is ever refused.
      for (const intent of own.input.question.shape.intents)
        if (intent.kind === "attack")
          for (const { id } of intent.targets)
            courses.push([...choices, { kind: "action_window", intent: intent.id, target: id }])
        else courses.push([...choices, { kind: "action_window", intent: intent.id, target: null }])
    }
    // Three actions in any order, one of them with two recipients, and an end after any of them.
    expect(ended).toBeGreaterThan(10)
    expect(turn.after([{ kind: "action_window", intent: "nothing", target: null }])).toBeNull()
  })
})
