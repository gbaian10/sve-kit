// @vitest-environment node
import { describe, expect, it } from "vitest"

import {
  type BattleObservation,
  type BattleUpdate,
  type HostMessage,
  type Seat,
  validateHostMessage,
  type ZoneView,
} from "../contract"
import { SCENES } from "./scenes"
import { scriptedSource } from "./scripted"

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
        expect("viewer" in message && message.viewer).toBe(seat)
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

  it.each(SCENES)("$id arranges a slot for each field card", ({ script, slots }) => {
    const [first, second] = baselineOf(script.p1).players
    expect(slots.p1?.field).toHaveLength(first.zones.field.count)
    expect(slots.p2?.field).toHaveLength(second.zones.field.count)
  })

  it("replays the seat's script to every listener until the source is closed", () => {
    const scene = SCENES[0]
    const source = scriptedSource(scene, "p2")
    const first: HostMessage[] = []
    const second: HostMessage[] = []
    source.listen((message) => first.push(message))
    source.listen((message) => second.push(message))
    expect(first).toEqual(scene.script.p2)
    expect(second).toEqual(first)
    source.close()
    const late: HostMessage[] = []
    source.listen((message) => late.push(message))
    expect(late).toEqual([])
  })
})
