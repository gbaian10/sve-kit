import { describe, expect, it } from "vitest"

import { parseDeckText } from "../deck/text"
import {
  chooseReading,
  type DeckCard,
  deckGroups,
  deckListText,
  guessFromDeck,
  withoutMarks,
  withPrinting,
} from "./deck-read"
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

  it("shows another printing in the pick and in that card's candidate, so confirming keeps it", () => {
    const slA = { ...A, no: "BP01-SL01", imageId: 2 }
    const shown = withPrinting(read("ok", [match(A, 0.9), match(B, 0.5)]), slA)
    expect(shown.pick).toBe(slA)
    expect(shown.alts.map((alt) => alt.card.no)).toEqual(["BP01-SL01", "BP01-002"])
    expect(shown.alts[0]?.raw).toBe(0.9)
  })
})

/** A read at (x, y) with a square box of side `side`. */
const at = (x: number, y: number, side: number, raw: number): DeckCard => ({
  ...read(raw >= 0.5 ? "ok" : "unsure", [match(A, raw)]),
  quad: [
    [x, y],
    [x + side, y],
    [x + side, y + side],
    [x, y + side],
  ],
})

describe("printed marks", () => {
  it("drops unreadable finds far smaller than the cards read around them", () => {
    const cards = [at(0, 0, 100, 0.7), at(150, 0, 100, 0.6), at(300, 0, 100, 0.65)]
    const circle = at(120, 150, 20, 0.25)
    const farCard = at(450, 0, 60, 0.3)
    expect(withoutMarks([...cards, circle, farCard])).toEqual([...cards, farCard])
  })

  it("keeps small finds that read well, and everything when nothing was read", () => {
    const small = at(120, 150, 20, 0.45)
    expect(withoutMarks([at(0, 0, 100, 0.7), small])).toContainEqual(small)
    const unread = [at(0, 0, 100, 0.3), at(120, 150, 20, 0.2)]
    expect(withoutMarks(unread)).toEqual(unread)
  })
})
