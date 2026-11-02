import { describe, expect, it } from "vitest"

import { card, lookup } from "../../test-utils/deck-cards"
import { averageCost, costCurve, groupEntries, typeCounts } from "./stats"

const infoOf = lookup(
  card("a", { cost: 1 }),
  card("b", { cost: 3, typeCode: "spell" }),
  card("c", { cost: 9, typeCode: "amulet" }),
  card("d", { cost: 3, name: "aa" }),
  card("e", { cost: null, typeCode: "follower", specialKinds: ["evolve"] }),
)
const costOf = (cardId: string) => infoOf(cardId)?.cost ?? null
const entries = [
  { cardId: "a", count: 3 },
  { cardId: "b", count: 2 },
  { cardId: "c", count: 1 },
  { cardId: "d", count: 1 },
  { cardId: "e", count: 2 },
]

describe("deck stats", () => {
  it("counts copies per cost with 8 and above in one slot, leaving costless cards out", () => {
    expect(costCurve(entries, costOf)).toEqual([0, 3, 0, 3, 0, 0, 0, 0, 1])
  })

  it("averages over the copies that have a cost", () => {
    expect(averageCost(entries, costOf)).toBeCloseTo((3 * 1 + 2 * 3 + 9 + 3) / 7)
    expect(averageCost([{ cardId: "e", count: 2 }], costOf)).toBeNull()
  })

  it("counts copies per type", () => {
    expect(typeCounts(entries, infoOf)).toEqual({ follower: 6, spell: 2, amulet: 1, other: 0 })
  })

  it("groups by type in follower, spell, amulet order, cheapest first inside a group", () => {
    const groups = groupEntries(entries, infoOf, "type")
    expect(groups.map((group) => [group.key, group.count])).toEqual([
      ["follower", 6],
      ["spell", 2],
      ["amulet", 1],
    ])
    expect(groups[0]?.entries.map((entry) => entry.cardId)).toEqual(["a", "d", "e"])
  })

  it("groups by cost slot with costless cards last", () => {
    const groups = groupEntries(entries, infoOf, "cost")
    expect(groups.map((group) => [group.key, group.entries.map((entry) => entry.cardId)])).toEqual([
      ["1", ["a"]],
      ["3", ["d", "b"]],
      ["8+", ["c"]],
      [null, ["e"]],
    ])
  })
})
