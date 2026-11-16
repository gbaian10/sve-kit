// @vitest-environment node
import { describe, expect, it } from "vitest"

import type { BattleObservation, Seat } from "../contract"
import { baseline, type SeatSpec, type TableSpec } from "../fixtures/build"
import { cardRef } from "../fixtures/cards"
import { tableModel as model } from "./table"

const seat = (spec: Partial<SeatSpec>): SeatSpec => ({
  life: 20,
  pp: 0,
  maxPp: 0,
  ep: 0,
  sep: 0,
  epCapacity: 0,
  sepAvailableFrom: null,
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

const TABLE: TableSpec = {
  turn: { active: "p2", first_player: "p1", phase: "main", elapsed_turns: [3, 2] },
  seats: [
    seat({
      life: 17,
      pp: 1,
      maxPp: 3,
      sepAvailableFrom: 2,
      hand: ["BP05-058", "BP20-062"],
      field: [
        { no: "BP20-064", slot: 3, evolved: "BP20-065", acted: true, keywords: ["guard"] },
        { no: "BP01-T10", slot: 0, counters: [["スタックカウンター", 2]] },
      ],
      cemetery: ["BP08-068", "SD04-002"],
      evolveDeck: { faceDown: ["BP05-053"], faceUp: ["BP15-114"] },
    }),
    seat({
      ep: 2,
      sep: 1,
      epCapacity: 3,
      sepAvailableFrom: 4,
      hand: ["BP13-026", "BP16-026", "BP13-035"],
      ex: ["BP16-031"],
    }),
  ],
}

/** The slots the scene lays its field cards in; the observation itself only orders them. */
const ARRANGED = {
  p1: { field: TABLE.seats[0].field.map((card) => card.slot) },
  p2: { field: TABLE.seats[1].field.map((card) => card.slot) },
}

const tableModel = (observation: BattleObservation) => model(observation, ARRANGED)

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

  it("draws evolve points as how many slots there are and how many are left", () => {
    const { self, opponent } = tableModel(observe(TABLE, "p1"))
    // No evolve point slots. Its super evolution point is spent and has been open since its
    // second turn: the spent one stays as a marker.
    expect(self.ep).toEqual({ current: 0, max: 0 })
    expect(self.sep).toEqual({ current: 0, max: 1, opensIn: 0 })
    // One of three used. It has begun two turns and may super evolve from its fourth.
    expect(opponent.ep).toEqual({ current: 2, max: 3 })
    expect(opponent.sep).toEqual({ current: 1, max: 1, opensIn: 2 })
  })

  it("shows no super evolution marker before any is handed out", () => {
    const before: TableSpec = {
      turn: { active: null, first_player: null, phase: null, elapsed_turns: [0, 0] },
      seats: [seat({}), seat({})],
    }
    const { self } = model(observe(before, "p1"))
    expect(self.sep).toEqual({ current: 0, max: 0, opensIn: 0 })
  })

  it("gives the viewer's own field cards the targets it is told they may attack", () => {
    const [first, second] = TABLE.seats
    const table: TableSpec = {
      ...TABLE,
      seats: [first, { ...second, field: [{ no: "BP13-030", slot: 2 }] }],
    }
    // By zone order: the first card may attack the leader, the second one opposing card only.
    // Place 7 of the opposing field is empty, so it names nothing.
    const hints = {
      p1: [
        { leader: true, followers: [] },
        { leader: false, followers: [0, 7] },
      ],
      p2: [{ leader: true, followers: [0] }],
    }
    const own = model(observe(table, "p1"), ARRANGED, hints)
    const target = own.opponent.field.find((card) => card !== null)?.handle
    expect(own.self.field.flatMap((card) => (card === null ? [] : [card.attack]))).toEqual([
      { leader: false, targets: [target] },
      { leader: true, targets: [] },
    ])
    // A seat is told nothing about what the other seat's cards may do.
    expect(own.opponent.field.every((card) => card === null || card.attack === null)).toBe(true)
    // Nothing is assumed without being told: no hints, no attackers.
    const untold = model(observe(table, "p1"), ARRANGED)
    expect(untold.self.field.every((card) => card === null || card.attack === null)).toBe(true)
  })

  it("lays a field card in the slot numbered like its place when nothing is arranged", () => {
    const { self } = model(observe(TABLE, "p1"))
    expect(self.field.map((card) => card?.face ?? null)).toEqual([
      cardRef("BP20-065"),
      cardRef("BP01-T10"),
      null,
      null,
      null,
    ])
  })

  it("takes the next free slot when the arranged one is taken or off the row", () => {
    const observation = observe(TABLE, "p1")
    const shared = model(observation, { p1: { field: [3, 3] } }).self.field
    expect(shared.map((card) => card !== null)).toEqual([true, false, false, true, false])
    expect(shared[3]?.face).toEqual(cardRef("BP20-065"))
    const outside = model(observation, { p1: { field: [9, 2] } }).self.field
    expect(outside.map((card) => card !== null)).toEqual([true, false, true, false, false])
  })

  it("keeps field cards in their arranged slots and leaves the other slots empty", () => {
    const { self } = tableModel(observe(TABLE, "p1"))
    expect(self.field).toHaveLength(5)
    expect(self.field.map((card) => card !== null)).toEqual([true, false, false, true, false])
    expect(self.field[0]).toMatchObject({
      face: cardRef("BP01-T10"),
      acted: false,
      stats: null,
      counters: [{ name: "スタックカウンター", amount: 2 }],
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

  it("has no cost for a hand card without a settled quote", () => {
    const observation = observe(TABLE, "p1")
    const [first, second] = observation.costs.cards
    if (first === undefined || second === undefined) throw new Error("two quotes expected")
    // The first quote still has something pending, so its total is left open; the second is gone.
    const { self } = tableModel({
      ...observation,
      costs: {
        ...observation.costs,
        cards: [{ ...first, cost: { ...first.cost, total_pp: null } }],
      },
    })
    expect(self.hand.cards.map((card) => card.cost)).toEqual([null, null])
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
