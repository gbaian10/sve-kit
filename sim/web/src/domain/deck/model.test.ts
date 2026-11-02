import { describe, expect, it } from "vitest"

import { card } from "../../test-utils/deck-cards"
import {
  copyLimit,
  countOf,
  createDeck,
  duplicateDeck,
  editDeck,
  placementOf,
  renameDeck,
  totalCount,
  withCount,
  withPrinting,
} from "./model"
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

  it("places leaders, evolve-deck cards, main-deck cards, and nothing for tokens", () => {
    expect(placementOf(card("l", { typeCode: "leader" }))).toBe("leader")
    expect(placementOf(card("e", { specialKinds: ["evolve"] }))).toBe("evolve")
    expect(placementOf(card("a", { specialKinds: ["advance"] }))).toBe("evolve")
    expect(placementOf(card("t", { specialKinds: ["token"] }))).toBe("none")
    expect(placementOf(card("s", { typeCode: "spell" }))).toBe("main")
    expect(placementOf(card("r", { typeCode: null, deckRole: "evolve" }))).toBe("evolve")
    expect(placementOf(card("u", { typeCode: null }))).toBe("none")
  })

  it("sets a card's copies, adding it last and dropping it at zero", () => {
    const main = sample().main
    expect(withCount(main, "c:3", 1)).toEqual([...main, { cardId: "c:3", count: 1 }])
    expect(withCount(main, "c:1", 2, "p:1")[0]).toEqual({
      cardId: "c:1",
      count: 2,
      printingId: "p:1",
    })
    expect(withCount(main, "c:2", 3)[1]).toEqual({ cardId: "c:2", count: 3, printingId: "p:2" })
    expect(withCount(main, "c:1", 0).map((entry) => entry.cardId)).toEqual(["c:2"])
    expect(countOf(main, "c:2")).toBe(2)
    expect(countOf(main, "c:9")).toBe(0)
  })

  it("chooses a printing or goes back to the default", () => {
    const main = sample().main
    expect(withPrinting(main, "c:1", "p:x")[0]).toEqual({
      cardId: "c:1",
      count: 3,
      printingId: "p:x",
    })
    expect(withPrinting(main, "c:2", undefined)[1]).toEqual({ cardId: "c:2", count: 2 })
  })

  it("limits copies by card name in the section only", () => {
    const deck = {
      ...sample(),
      main: [{ cardId: "c:1", count: 2 }],
      evolve: [{ cardId: "c:3", count: 3 }],
    }
    const names: Record<string, string> = { "c:1": "Elf", "c:2": "Elf", "c:3": "Elf" }
    const nameOf = (id: string) => names[id]
    expect(copyLimit(deck, "main", { cardId: "c:2", name: "Elf" }, nameOf)).toBe(1)
    expect(copyLimit(deck, "main", { cardId: "c:1", name: "Elf" }, nameOf)).toBe(3)
    expect(copyLimit(deck, "evolve", { cardId: "c:2", name: "Elf" }, nameOf)).toBe(0)
  })

  it("records every edit as a newer revision of the same deck", () => {
    const deck = sample()
    const edited = editDeck(deck, { leader: { cardId: "c:l" } }, LATER)
    expect(edited).toMatchObject({ id: deck.id, revision: 2, leader: { cardId: "c:l" } })
    expect(edited.updatedAt).toBe(LATER.toISOString())
  })
})
