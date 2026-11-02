import { describe, expect, it } from "vitest"

import { createDeck } from "./model"
import { deckToText, parseDeckText } from "./text"
import type { Deck } from "./types"

const labels = { leader: "主戰者", main: "主牌組", evolve: "進化牌組", side: "候補" }
const numbers: Record<string, string> = {
  "c:leader": "BP20-LD01",
  "c:1": "BP01-001",
  "c:2": "BP01-002",
  "c:evo": "BP01-010",
  "c:side": "BP02-050",
}
const lookup = {
  cardNoOf: (entry: { readonly cardId: string; readonly printingId?: string }) =>
    entry.printingId ?? numbers[entry.cardId],
  nameOf: (cardId: string) => `name ${cardId}`,
}

function deck(change: Partial<Deck> = {}): Deck {
  return {
    ...createDeck(
      { name: "精靈快攻", edition: "jp", basis: { kind: "class", classCode: "elf" } },
      new Date("2026-11-03T00:00:00.000Z"),
    ),
    ...change,
  }
}

describe("deck text", () => {
  it("writes a name, a heading per part and one line per card", () => {
    const text = deckToText(
      deck({
        leader: { cardId: "c:leader" },
        main: [
          { cardId: "c:1", count: 3 },
          { cardId: "c:2", count: 2, printingId: "BP01-002P" },
        ],
        evolve: [{ cardId: "c:evo", count: 1 }],
        side: [{ cardId: "c:side", count: 1 }],
      }),
      lookup,
      labels,
    )
    expect(text).toBe(
      [
        "# 精靈快攻",
        "",
        "## 主戰者",
        "1 BP20-LD01 name c:leader",
        "",
        "## 主牌組 (5)",
        "3 BP01-001 name c:1",
        "2 BP01-002P name c:2",
        "",
        "## 進化牌組 (1)",
        "1 BP01-010 name c:evo",
        "",
        "## 候補 (1)",
        "1 BP02-050 name c:side",
        "",
      ].join("\n"),
    )
  })

  it("leaves out an empty side list but keeps empty decks", () => {
    const text = deckToText(deck(), lookup, labels)
    expect(text).toBe("# 精靈快攻\n\n## 主牌組 (0)\n\n## 進化牌組 (0)\n")
  })

  it("reads back its own output: name, counts, card numbers and the side list", () => {
    const written = deckToText(
      deck({
        leader: { cardId: "c:leader" },
        main: [{ cardId: "c:1", count: 3 }],
        side: [{ cardId: "c:side", count: 2 }],
      }),
      lookup,
      labels,
    )
    const parsed = parseDeckText(written)
    expect(parsed.name).toBe("精靈快攻")
    expect(parsed.unrecognized).toEqual([])
    expect(parsed.cards.map(({ count, cardNo, side }) => ({ count, cardNo, side }))).toEqual([
      { count: 1, cardNo: "BP20-LD01", side: false },
      { count: 3, cardNo: "BP01-001", side: false },
      { count: 2, cardNo: "BP02-050", side: true },
    ])
  })

  it("accepts common list spellings and reports what it cannot read", () => {
    const parsed = parseDeckText(
      [
        "3x BP01-001",
        "BP01-002 ×2",
        "BP01-003",
        "2 BP01-004 名前",
        "## Maybeboard",
        "1 BP02-001",
        "hello",
        "0 BP01-005",
      ].join("\n"),
    )
    expect(parsed.cards.map(({ count, cardNo, side }) => [count, cardNo, side])).toEqual([
      [3, "BP01-001", false],
      [2, "BP01-002", false],
      [1, "BP01-003", false],
      [2, "BP01-004", false],
      [1, "BP02-001", true],
    ])
    expect(parsed.unrecognized).toEqual([
      { line: 7, text: "hello" },
      { line: 8, text: "0 BP01-005" },
    ])
    expect(parsed.name).toBeUndefined()
  })
})
