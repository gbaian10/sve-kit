import { describe, expect, it } from "vitest"

import { parseDeck } from "./parse"

const valid = {
  id: "d1",
  name: "Deck",
  edition: "jp",
  basis: { kind: "class", classCode: "forest" },
  leader: { cardId: "c:l", printingId: "p:l" },
  main: [{ cardId: "c:1", count: 3, printingId: "p:1" }],
  evolve: [{ cardId: "c:2", count: 1 }],
  side: [],
  createdAt: "2026-11-03T01:00:00.000Z",
  updatedAt: "2026-11-03T01:00:00.000Z",
  revision: 4,
}

describe("parseDeck", () => {
  it("accepts a complete record, with or without a leader and printings", () => {
    expect(parseDeck(valid)).toEqual(valid)
    expect(parseDeck({ ...valid, leader: null })?.leader).toBeNull()
    expect(
      parseDeck({ ...valid, basis: { kind: "title", titleCode: "umamusume" } })?.basis,
    ).toEqual({
      kind: "title",
      titleCode: "umamusume",
    })
  })

  it("drops fields it does not know", () => {
    const parsed = parseDeck({ ...valid, extra: 1, basis: { ...valid.basis, extra: 2 } })
    expect(parsed).toEqual(valid)
  })

  it.each([
    ["not an object", "deck"],
    ["no id", { ...valid, id: undefined }],
    ["unknown edition", { ...valid, edition: "zh" }],
    ["unknown basis", { ...valid, basis: { kind: "set", code: "bp01" } }],
    ["basis without its code", { ...valid, basis: { kind: "class" } }],
    ["leader of the wrong shape", { ...valid, leader: "c:l" }],
    ["section that is not a list", { ...valid, main: {} }],
    ["entry without a card", { ...valid, main: [{ count: 1 }] }],
    ["zero copies", { ...valid, main: [{ cardId: "c:1", count: 0 }] }],
    ["fractional copies", { ...valid, evolve: [{ cardId: "c:1", count: 1.5 }] }],
    ["revision below 1", { ...valid, revision: 0 }],
    ["time that is not text", { ...valid, updatedAt: 5 }],
    ["time that is not a date", { ...valid, createdAt: "not-a-date" }],
  ])("refuses a record with %s", (_label, value) => {
    expect(parseDeck(value)).toBeNull()
  })
})
