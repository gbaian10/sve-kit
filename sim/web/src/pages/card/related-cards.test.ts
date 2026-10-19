// @vitest-environment node
import { describe, expect, it } from "vitest"

import type { CardSummary, SelectedText } from "../../data"
import {
  type CardKind,
  evolveDirection,
  isEvolvePair,
  mentionedCards,
  relatedGroups,
  representatives,
} from "./related-cards"

const summary = (cardId: string, cardNo: string, region: "jp" | "en" = "jp"): CardSummary => ({
  cardId,
  printingId: `p:${cardNo}`,
  faceId: `f:${cardId}`,
  cardNo,
  region,
  classCode: null,
  name: { original: { lang: "ja", text: cardNo }, translations: {} },
  cost: null,
  attack: null,
  defense: null,
})

const related = (from: string, ...to: string[]) =>
  to.map((target, index) => ({
    id: `related:${String(index)}`,
    from_card_id: from,
    to_card_id: target,
    relation: "official_unspecified",
    suggested_count: null,
    applicable_regions: null,
  }))

const text = (
  references: readonly { readonly termId: string; readonly cardIds: readonly string[] }[],
  cards: readonly CardSummary[],
  where: "original" | "translated" | "source" | "sourceSections" = "original",
): SelectedText => {
  const annotated = {
    unit: { text: "x".repeat(references.length), lang: "ja" },
    annotation: {
      occurrences: references.map(({ termId }, index) => ({
        reference: { kind: "card_name", term_id: termId },
        ranges: [{ start: index, end: index + 1 }],
        bold: true,
      })),
    },
  }
  const plain = { unit: { text: "", lang: "ja" }, annotation: null }
  return {
    original: where === "original" ? annotated : plain,
    ...(where === "translated" ? { translated: annotated } : {}),
    ...(where === "source" ? { source: annotated } : {}),
    ...(where === "sourceSections" ? { sourceSections: [plain, annotated] } : {}),
    concepts: references.map(({ termId, cardIds }) => ({
      id: termId,
      category: "card_name",
      explanations: [],
      card_ids: [...cardIds],
    })),
    explanations: [],
    cards,
  }
}

describe("representatives", () => {
  it("keeps one printing per card, preferring the shown edition", () => {
    const jp = summary("c:a", "BP01-001")
    const en = summary("c:a", "BP01-001EN", "en")
    expect(representatives([jp, en], "en")).toEqual([en])
    expect(representatives([en, jp], "jp")).toEqual([jp])
    expect(representatives([jp], "en")).toEqual([jp])
  })
})

describe("mentionedCards", () => {
  it("lists the cards named by confirmed references in text order, once each", () => {
    const fairy = summary("c:fairy", "PR-108")
    const other = summary("c:other", "BP01-002")
    const value = text(
      [
        { termId: "term:name.fairy", cardIds: ["c:fairy"] },
        { termId: "term:name.other", cardIds: ["c:other"] },
        { termId: "term:name.fairy", cardIds: ["c:fairy"] },
        { termId: "term:name.unlinked", cardIds: [] },
      ],
      [other, fairy],
    )
    expect(mentionedCards([value], "jp").map((card) => card.cardNo)).toEqual(["PR-108", "BP01-002"])
  })

  it("reads the translation, the Japanese source and its sections when the original carries no annotation", () => {
    const fairy = summary("c:fairy", "PR-108")
    for (const where of ["translated", "source", "sourceSections"] as const) {
      const value = text([{ termId: "term:name.fairy", cardIds: ["c:fairy"] }], [fairy], where)
      expect(mentionedCards([value], "jp")).toEqual([fairy])
    }
  })

  it("keeps the reading order across the source text and its sections", () => {
    const a = summary("c:a", "BP01-001")
    const b = summary("c:b", "BP01-002")
    const sections = text([{ termId: "term:name.b", cardIds: ["c:b"] }], [a, b], "sourceSections")
    const source = text([{ termId: "term:name.a", cardIds: ["c:a"] }], [a, b], "source")
    const value: SelectedText = {
      ...sections,
      ...(source.source ? { source: source.source } : {}),
      concepts: [...sections.concepts, ...source.concepts],
    }
    expect(mentionedCards([value], "jp").map((card) => card.cardNo)).toEqual([
      "BP01-001",
      "BP01-002",
    ])
  })

  it("skips unresolved texts and cards the snapshot does not hold in the shown edition", () => {
    const en = summary("c:fairy", "PR-083EN", "en")
    const value = text([{ termId: "term:name.fairy", cardIds: ["c:fairy"] }], [en])
    expect(mentionedCards([null, undefined, value], "jp")).toEqual([en])
    expect(mentionedCards([text([{ termId: "t", cardIds: ["c:missing"] }], [])], "jp")).toEqual([])
  })
})

type Flag = "evolve" | "advance" | "token" | "spell"
const kind = (name: string | undefined, ...flags: Flag[]): CardKind => ({
  name,
  evolve: flags.includes("evolve"),
  advance: flags.includes("advance"),
  token: flags.includes("token"),
  follower: !flags.includes("spell"),
})

describe("isEvolvePair", () => {
  it("pairs an evolve card with the same-named card on the other side, in either order", () => {
    expect(
      isEvolvePair(kind("侮蔑の絶傑・ガルミーユ"), kind("侮蔑の絶傑・ガルミーユ", "evolve")),
    ).toBe(true)
    expect(
      isEvolvePair(kind("侮蔑の絶傑・ガルミーユ", "evolve"), kind("侮蔑の絶傑・ガルミーユ")),
    ).toBe(true)
    expect(isEvolvePair(kind(" 名前 ", "evolve"), kind("名前"))).toBe(true)
  })

  it("rejects differently named partners of an evolve card: tokens, mentions and other relations", () => {
    expect(isEvolvePair(kind("仮面人形", "evolve"), kind("操り人形", "token"))).toBe(false)
    expect(
      isEvolvePair(kind("至高の凌駕", "spell"), kind("唯我の絶傑・マゼルベイン", "evolve")),
    ).toBe(false)
    expect(isEvolvePair(kind("エイシンフラッシュ"), kind("にんじん", "evolve"))).toBe(false)
    expect(isEvolvePair(kind(undefined, "evolve"), kind(undefined))).toBe(false)
  })

  it("rejects two evolve cards and two plain cards even with the same name", () => {
    expect(isEvolvePair(kind("名前", "evolve"), kind("名前", "evolve"))).toBe(false)
    expect(isEvolvePair(kind("名前"), kind("名前"))).toBe(false)
    expect(isEvolvePair(kind("名前"), kind("名前", "token"))).toBe(false)
  })

  it("pairs an advance card with a plain follower whatever its name, never with a token, spell or evolve card", () => {
    expect(isEvolvePair(kind("楽園の開闢・タケツミ", "advance"), kind("華の大将・タケツミ"))).toBe(
      true,
    )
    expect(isEvolvePair(kind("華の大将・タケツミ"), kind("楽園の開闢・タケツミ", "advance"))).toBe(
      true,
    )
    expect(isEvolvePair(kind("A", "advance"), kind("B", "token"))).toBe(false)
    expect(isEvolvePair(kind("A", "advance"), kind("B", "spell"))).toBe(false)
    expect(isEvolvePair(kind("A", "advance"), kind("B", "evolve"))).toBe(false)
    expect(isEvolvePair(kind("A", "advance"), kind("B", "advance"))).toBe(false)
  })
})

describe("evolveDirection", () => {
  it("lists what evolve and advance cards evolve from and what other cards evolve into", () => {
    expect(evolveDirection(kind("A", "evolve"))).toBe("from")
    expect(evolveDirection(kind("A", "advance"))).toBe("from")
    expect(evolveDirection(kind("A"))).toBe("into")
    expect(evolveDirection(kind("A", "token"))).toBe("into")
  })
})

describe("relatedGroups", () => {
  const cards = new Map(
    [
      summary("c:base", "BP05-052"),
      summary("c:evolve", "BP05-053"),
      summary("c:evolve2", "BP05-055"),
      summary("c:token", "PR-108"),
      summary("c:other", "BP05-070"),
      summary("c:another", "BP05-010"),
      summary("c:advance", "BP14-019"),
      summary("c:advanceBase", "BP14-018"),
    ].map((card) => [card.cardId, card]),
  )
  const kinds: Record<string, CardKind> = {
    "c:base": kind("Galmieux"),
    "c:evolve": kind("Galmieux", "evolve"),
    "c:evolve2": kind("Another evolve", "evolve"),
    "c:token": kind("Puppet", "token"),
    "c:other": kind("Other"),
    "c:another": kind("Another"),
    "c:advance": kind("Takeyomi, Paradise", "advance"),
    "c:advanceBase": kind("Takeyomi, Captain"),
  }
  const representative = (id: string) => cards.get(id)
  const kindOf = (id: string) => kinds[id]
  const card = (id: string): CardSummary => {
    const found = cards.get(id)
    if (!found) throw new Error(`missing synthetic card ${id}`)
    return found
  }
  const numbers = (list: readonly CardSummary[]) => list.map((entry) => entry.cardNo)

  it("lists what a plain card evolves into: only its same-named evolve card", () => {
    const groups = relatedGroups({
      cardId: "c:base",
      related: related("c:base", "c:other", "c:evolve2", "c:evolve"),
      mentions: [],
      representative,
      kind: kindOf,
    })
    expect(groups.evolve.direction).toBe("into")
    expect(numbers(groups.evolve.cards)).toEqual(["BP05-053"])
    expect(numbers(groups.otherOfficial)).toEqual(["BP05-055", "BP05-070"])
  })

  it("lists what an evolve card evolves from and sends its tokens to the references", () => {
    const groups = relatedGroups({
      cardId: "c:evolve",
      related: related("c:evolve", "c:token", "c:base"),
      mentions: [],
      representative,
      kind: kindOf,
    })
    expect(groups.evolve.direction).toBe("from")
    expect(numbers(groups.evolve.cards)).toEqual(["BP05-052"])
    expect(numbers(groups.otherOfficial)).toEqual(["PR-108"])
  })

  it("pairs an advance card with its follower in both directions", () => {
    const fromAdvance = relatedGroups({
      cardId: "c:advance",
      related: related("c:advance", "c:advanceBase", "c:token"),
      mentions: [],
      representative,
      kind: kindOf,
    })
    expect(fromAdvance.evolve).toEqual({ direction: "from", cards: [card("c:advanceBase")] })
    expect(numbers(fromAdvance.otherOfficial)).toEqual(["PR-108"])
    const fromBase = relatedGroups({
      cardId: "c:advanceBase",
      related: related("c:advanceBase", "c:advance"),
      mentions: [],
      representative,
      kind: kindOf,
    })
    expect(fromBase.evolve).toEqual({ direction: "into", cards: [card("c:advance")] })
  })

  it("lists a card once: evolve first, then the text's mentions, then other official relations", () => {
    const groups = relatedGroups({
      cardId: "c:base",
      related: related("c:base", "c:token", "c:evolve", "c:other", "c:token"),
      mentions: [card("c:token"), card("c:evolve"), card("c:another")],
      representative,
      kind: kindOf,
    })
    expect(numbers(groups.evolve.cards)).toEqual(["BP05-053"])
    expect(numbers(groups.mentions)).toEqual(["PR-108", "BP05-010"])
    expect(numbers(groups.otherOfficial)).toEqual(["BP05-070"])
  })

  it("never lists the card itself, a card without a representative printing, or a pair whose kind is unknown", () => {
    const groups = relatedGroups({
      cardId: "c:base",
      related: related("c:base", "c:base", "c:unknown", "c:other", "c:evolve"),
      mentions: [card("c:base")],
      representative,
      kind: (id) => (id === "c:evolve" ? undefined : kinds[id]),
    })
    expect(groups.evolve.cards).toEqual([])
    expect(groups.mentions).toEqual([])
    expect(numbers(groups.otherOfficial)).toEqual(["BP05-053", "BP05-070"])
  })
})
