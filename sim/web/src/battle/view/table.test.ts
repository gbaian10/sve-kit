// @vitest-environment node
import { describe, expect, it } from "vitest"

import type { BattleObservation, Seat } from "../contract"
import { baseline, type SeatSpec, type TableSpec } from "../fixtures/build"
import { cardRef } from "../fixtures/cards"
import { type ProvisionalPoints, tableModel as model } from "./table"

const seat = (spec: Partial<SeatSpec>): SeatSpec => ({
  life: 20,
  pp: 0,
  maxPp: 0,
  ep: 0,
  sep: 0,
  epMax: 0,
  sepMax: 1,
  sepOpensIn: 0,
  deck: 30,
  hand: [],
  field: [],
  ex: [],
  cemetery: [],
  banish: [],
  evolveDeck: { faceDown: [], faceUp: [] },
  ...spec,
})

const observe = (table: TableSpec, viewer: Seat): BattleObservation =>
  baseline(table, viewer).observation

const pointsOf = (table: TableSpec): Record<Seat, ProvisionalPoints> => {
  const [first, second] = table.seats
  return { p1: first, p2: second }
}

const TABLE: TableSpec = {
  turn: { active: "p2", first_player: "p1", phase: "main", elapsed_turns: [3, 2] },
  seats: [
    seat({
      life: 17,
      pp: 1,
      maxPp: 3,
      hand: ["BP05-058", "BP20-062"],
      field: [
        { no: "BP20-064", slot: 3, evolved: "BP20-065", acted: true, keywords: ["guard"] },
        { no: "BP20-067", slot: 0, counters: [["スタック", 2]] },
      ],
      cemetery: ["BP08-068", "SD04-002"],
      evolveDeck: { faceDown: ["BP05-053"], faceUp: ["BP15-114"] },
    }),
    seat({
      ep: 2,
      sep: 1,
      epMax: 3,
      sepOpensIn: 2,
      hand: ["BP13-026", "BP16-026", "BP13-035"],
      ex: ["BP16-031"],
    }),
  ],
}

const tableModel = (observation: BattleObservation) => model(observation, pointsOf(TABLE))

describe("tableModel", () => {
  it("puts the viewer's seat on its own side whichever seat it is", () => {
    const first = tableModel(observe(TABLE, "p1"))
    const second = tableModel(observe(TABLE, "p2"))
    expect([first.self.seat, first.opponent.seat]).toEqual(["p1", "p2"])
    expect([second.self.seat, second.opponent.seat]).toEqual(["p2", "p1"])
    expect(first.self).toMatchObject({ life: 17, pp: 1, maxPp: 3, first: true, active: false })
    expect(first.opponent).toMatchObject({ first: false, active: true, turns: 2 })
    expect(first.phase).toBe("main")
  })

  it("draws evolve points as how many there were and how many are left", () => {
    const { self, opponent } = tableModel(observe(TABLE, "p1"))
    // The seat without evolve points has none to show; its super evolution is open.
    expect(self.ep).toEqual({ current: 0, max: 0 })
    expect(self.sep).toEqual({ current: 0, max: 1, opensIn: 0 })
    // One of three used, and two of the seat's own turns before a super evolution.
    expect(opponent.ep).toEqual({ current: 2, max: 3 })
    expect(opponent.sep).toEqual({ current: 1, max: 1, opensIn: 2 })
  })

  it("keeps field cards in their slots and leaves the other slots empty", () => {
    const { self } = tableModel(observe(TABLE, "p1"))
    expect(self.field).toHaveLength(5)
    expect(self.field.map((card) => card !== null)).toEqual([true, false, false, true, false])
    expect(self.field[0]).toMatchObject({
      face: cardRef("BP20-067"),
      acted: false,
      stats: null,
      counters: [{ name: "スタック", amount: 2 }],
    })
  })

  it("shows an evolved follower by its evolve card with the authority's numbers", () => {
    const { self } = tableModel(observe(TABLE, "p1"))
    expect(self.field[3]).toMatchObject({
      card: cardRef("BP20-064"),
      face: cardRef("BP20-065"),
      evolution: "evolved",
      acted: true,
      stats: { attack: 3, life: 3 },
      keywords: ["guard"],
    })
  })

  it("knows its own hand by card and the other hand by size only", () => {
    const first = tableModel(observe(TABLE, "p1"))
    expect(first.self.hand.cards.map((card) => card.face)).toEqual([
      cardRef("BP05-058"),
      cardRef("BP20-062"),
    ])
    expect(first.self.hand.cards.map((card) => card.cost)).toEqual([1, 3])
    expect(first.opponent.hand).toEqual({ count: 3, cards: [] })
    expect(first.opponent.ex[0]?.face).toEqual(cardRef("BP16-031"))
  })

  it("counts the whole evolve deck and names only what the seat may know", () => {
    const own = tableModel(observe(TABLE, "p1")).self.evolveDeck
    const seen = tableModel(observe(TABLE, "p2")).opponent.evolveDeck
    expect(own.count).toBe(2)
    expect(own.cards.map((card) => [card.face, card.faceDown])).toEqual([
      [cardRef("BP15-114"), false],
      [cardRef("BP05-053"), true],
    ])
    expect(seen).toMatchObject({ count: 2, cards: [{ face: cardRef("BP15-114") }] })
  })

  it("orders a pile by position, so the last card is the one on top", () => {
    const { self } = tableModel(observe(TABLE, "p1"))
    expect(self.cemetery.cards.map((card) => card.face)).toEqual([
      cardRef("BP08-068"),
      cardRef("SD04-002"),
    ])
    expect(self.deck).toEqual({ count: 30, cards: [] })
  })

  it("still draws a card whose slot is unknown or already taken", () => {
    const observation: BattleObservation = observe(TABLE, "p1")
    const field = observation.players[0].zones.field
    const moved = field.cards.map((member) => ({ ...member, position: 0 }))
    const [kept, clashing] = moved
    if (kept === undefined || clashing === undefined) throw new Error("two field cards expected")
    const { self } = tableModel({
      ...observation,
      players: [
        {
          ...observation.players[0],
          zones: {
            ...observation.players[0].zones,
            field: { ...field, cards: [kept, { ...clashing, position: null }] },
          },
        },
        observation.players[1],
      ],
    })
    expect(self.field.filter((card) => card !== null)).toHaveLength(2)
    expect(self.field).toHaveLength(5)
  })

  it("ignores a zone member that has no object behind it", () => {
    const observation = observe(TABLE, "p1")
    const { self } = tableModel({ ...observation, objects: {} })
    expect(self.hand).toEqual({ count: 2, cards: [] })
    expect(self.field.every((card) => card === null)).toBe(true)
  })
})
