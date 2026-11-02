import { describe, expect, it } from "vitest"

import { createDeck, duplicateDeck, renameDeck, totalCount } from "./model"
import type { Deck } from "./types"

const NOW = new Date("2026-11-03T01:00:00.000Z")
const LATER = new Date("2026-11-03T02:30:00.000Z")

function sample(): Deck {
  return {
    ...createDeck({ name: "A", edition: "jp", basis: { kind: "class", classCode: "forest" } }, NOW),
    main: [
      { cardId: "c:1", count: 3 },
      { cardId: "c:2", count: 2, printingId: "p:2" },
    ],
  }
}

describe("deck model", () => {
  it("starts empty at revision 1 with equal creation and update times", () => {
    const deck = createDeck(
      { name: "A", edition: "en", basis: { kind: "title", titleCode: "t" } },
      NOW,
    )
    expect(deck).toMatchObject({
      name: "A",
      edition: "en",
      leader: null,
      main: [],
      evolve: [],
      side: [],
      revision: 1,
      createdAt: NOW.toISOString(),
      updatedAt: NOW.toISOString(),
    })
    expect(deck.id).not.toBe("")
  })

  it("gives every deck its own id", () => {
    const first = sample()
    const second = sample()
    expect(first.id).not.toBe(second.id)
  })

  it("renames in place of identity: same id, newer time, next revision", () => {
    const deck = sample()
    const renamed = renameDeck(deck, "B", LATER)
    expect(renamed).toMatchObject({
      id: deck.id,
      name: "B",
      revision: 2,
      createdAt: deck.createdAt,
    })
    expect(renamed.updatedAt).toBe(LATER.toISOString())
    expect(renamed.main).toEqual(deck.main)
  })

  it("duplicates the cards into a new deck with a fresh history", () => {
    const deck = renameDeck(sample(), "B", NOW)
    const copy = duplicateDeck(deck, "B copy", LATER)
    expect(copy.id).not.toBe(deck.id)
    expect(copy).toMatchObject({ name: "B copy", revision: 1, createdAt: LATER.toISOString() })
    expect(copy.main).toEqual(deck.main)
    expect(copy.basis).toEqual(deck.basis)
  })

  it("counts copies, not entries", () => {
    expect(totalCount(sample().main)).toBe(5)
    expect(totalCount([])).toBe(0)
  })
})
