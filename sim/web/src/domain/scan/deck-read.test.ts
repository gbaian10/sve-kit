import { describe, expect, it } from "vitest"

import { parseDeckText } from "../deck/text"
import { chooseReading, type DeckCard, deckGroups, deckListText, guessFromDeck } from "./deck-read"
import type { CardMatch, ScanCard } from "./search"

const card = (id: string, no: string): ScanCard => ({
  no,
  cardId: id,
  imageId: 1,
  face: 0,
  name: `name ${id}`,
  artGroup: "a",
  classCode: "",
})
const A = card("a", "BP01-001")
const B = card("b", "BP01-002")
const C = card("c", "BP01-003")
const match = (c: ScanCard, raw: number): CardMatch => ({ card: c, raw, score: raw })
const read = (status: DeckCard["status"], alts: CardMatch[]): DeckCard => ({
  quad: [
    [0, 0],
    [1, 0],
    [1, 1],
    [0, 1],
  ],
  crop: new Uint8ClampedArray(0),
  alts,
  pick: alts[0]?.card ?? null,
  status,
  gap: 0,
})

describe("deck reading", () => {
  it("takes the sideways reading only when it is clearly better", () => {
    expect(chooseReading([match(A, 0.8)], null)).toBe("upright")
    expect(chooseReading([match(A, 0.8)], [match(B, 0.81)])).toBe("upright")
    expect(chooseReading([match(A, 0.8)], [match(B, 0.83)])).toBe("sideways")
    expect(chooseReading([], [match(B, 0.3)])).toBe("sideways")
  })

  it("guesses an unsure card from cards recognised elsewhere in the same photo", () => {
    const cards = guessFromDeck([
      read("ok", [match(A, 0.9)]),
      read("unsure", [match(C, 0.45), match(A, 0.41)]),
      read("unsure", [match(C, 0.45), match(A, 0.3)]),
    ])
    expect(cards[1]).toMatchObject({ status: "guess", pick: A })
    expect(cards[1]?.gap).toBeCloseTo(0.04)
    expect(cards[2]).toMatchObject({ status: "unsure", pick: C })
  })

  it("groups picked cards by copies, leaves unsure and removed ones out, and lists them for import", () => {
    const groups = deckGroups([
      read("ok", [match(B, 0.9)]),
      null,
      read("ok", [match(A, 0.9)]),
      read("guess", [match(A, 0.6)]),
      read("unsure", [match(C, 0.3)]),
    ])
    expect(groups.map((group) => [group.card.no, group.at])).toEqual([
      ["BP01-001", [2, 3]],
      ["BP01-002", [0]],
    ])
    const text = deckListText(groups, "Deck")
    expect(text).toBe("# Deck\n2 BP01-001 name a\n1 BP01-002 name b\n")
    // The import page reads the same list back.
    const parsed = parseDeckText(text)
    expect(parsed.name).toBe("Deck")
    expect(parsed.cards.map((entry) => [entry.count, entry.cardNo])).toEqual([
      [2, "BP01-001"],
      [1, "BP01-002"],
    ])
    expect(deckListText([], "Deck")).toBe("")
  })
})
