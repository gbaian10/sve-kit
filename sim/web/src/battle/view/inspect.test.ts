// @vitest-environment node
import { describe, expect, it } from "vitest"

import type { BattleObservation, Seat } from "../contract"
import { type SeatSpec, type TableSpec, update } from "../fixtures/build"
import { cardRef } from "../fixtures/cards"
import { inspect } from "./inspect"
import { tableModel } from "./table"

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

const TABLE: TableSpec = {
  turn: { active: "p1", first_player: "p1", phase: "main", elapsed_turns: [3, 2] },
  seats: [
    seat({
      hand: ["BP05-058"],
      field: [{ no: "BP20-064", slot: 0, evolved: "BP20-065", acted: true, stats: [4, 1] }],
      cemetery: ["BP08-068"],
    }),
    seat({ ex: ["BP16-031"], hand: ["BP13-026"] }),
  ],
}

const observe = (viewer: Seat): BattleObservation => update(TABLE, viewer, 0).observation

describe("inspect", () => {
  it("says whose zone a card lies in and its current state", () => {
    const observation = observe("p1")
    const { self, opponent } = tableModel(observation)
    const onField = self.field[0]
    if (onField == null) throw new Error("the field card is missing")
    const state = inspect(observation, onField.handle)
    expect(state).toMatchObject({ seat: "p1", zone: "field" })
    expect(state?.card).toMatchObject({ acted: true, stats: { attack: 4, life: 1 } })
    // An evolved follower shows its evolve card; the follower itself lies underneath.
    expect(state?.card.face).toEqual(cardRef("BP20-065"))
    // The type is the one the card has now: the evolve card's, not the follower's under it.
    expect(state?.type).toBe("フォロワー・エボルヴ")
    expect(state?.under).toEqual(cardRef("BP20-064"))

    const inEx = opponent.ex[0]
    if (inEx == null) throw new Error("the EX card is missing")
    expect(inspect(observation, inEx.handle)).toMatchObject({ seat: "p2", zone: "ex", under: null })
    const buried = self.cemetery.cards[0]
    if (buried === undefined) throw new Error("the cemetery card is missing")
    expect(inspect(observation, buried.handle)).toMatchObject({ seat: "p1", zone: "cemetery" })
  })

  it("knows nothing about a card the seat cannot identify", () => {
    const own = tableModel(observe("p2")).self.hand.cards[0]
    if (own === undefined) throw new Error("the hand card is missing")
    // The other seat's name for that card means nothing here, and neither does an invented one.
    expect(inspect(observe("p1"), own.handle)).toBeNull()
    expect(inspect(observe("p1"), "nobody")).toBeNull()
  })
})
