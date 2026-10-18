// @vitest-environment node
import { describe, expect, it } from "vitest"

import type { CardSummary, SelectedText } from "../../data"
import { isEvolveKind, mentionedCards, relatedGroups, representatives } from "./related-cards"

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
  where: "original" | "translated" | "source" = "original",
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

  it("reads the translation and the Japanese source when the original carries no annotation", () => {
    const fairy = summary("c:fairy", "PR-108")
    for (const where of ["translated", "source"] as const) {
      const value = text([{ termId: "term:name.fairy", cardIds: ["c:fairy"] }], [fairy], where)
      expect(mentionedCards([value], "jp")).toEqual([fairy])
    }
  })

  it("skips unresolved texts and cards the snapshot does not hold in the shown edition", () => {
    const en = summary("c:fairy", "PR-083EN", "en")
    const value = text([{ termId: "term:name.fairy", cardIds: ["c:fairy"] }], [en])
    expect(mentionedCards([null, undefined, value], "jp")).toEqual([en])
    expect(mentionedCards([text([{ termId: "t", cardIds: ["c:missing"] }], [])], "jp")).toEqual([])
  })
})

describe("isEvolveKind", () => {
  it("recognises evolve and advance cards only", () => {
    expect(isEvolveKind(["evolve"])).toBe(true)
    expect(isEvolveKind(["advance"])).toBe(true)
    expect(isEvolveKind(["token"])).toBe(false)
    expect(isEvolveKind([])).toBe(false)
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
    ].map((card) => [card.cardId, card]),
  )
  const representative = (id: string) => cards.get(id)
  const card = (id: string): CardSummary => {
    const found = cards.get(id)
    if (!found) throw new Error(`missing synthetic card ${id}`)
    return found
  }
  const evolveKind = (id: string) => id.startsWith("c:evolve")

  it("puts official relations with an evolve card on either end into the evolve group", () => {
    const groups = relatedGroups({
      cardId: "c:base",
      related: related("c:base", "c:other", "c:evolve2", "c:evolve"),
      mentions: [],
      representative,
      evolveKind,
    })
    expect(groups.evolve.map((card) => card.cardNo)).toEqual(["BP05-053", "BP05-055"])
    expect(groups.otherOfficial.map((card) => card.cardNo)).toEqual(["BP05-070"])
    const fromEvolve = relatedGroups({
      cardId: "c:evolve",
      related: related("c:evolve", "c:base"),
      mentions: [],
      representative,
      evolveKind,
    })
    expect(fromEvolve.evolve.map((card) => card.cardNo)).toEqual(["BP05-052"])
    expect(fromEvolve.otherOfficial).toEqual([])
  })

  it("lists a card once: evolve first, then the text's mentions, then other official relations", () => {
    const groups = relatedGroups({
      cardId: "c:base",
      related: related("c:base", "c:token", "c:evolve", "c:other", "c:token"),
      mentions: [card("c:token"), card("c:evolve"), card("c:another")],
      representative,
      evolveKind,
    })
    expect(groups.evolve.map((card) => card.cardNo)).toEqual(["BP05-053"])
    expect(groups.mentions.map((card) => card.cardNo)).toEqual(["PR-108", "BP05-010"])
    expect(groups.otherOfficial.map((card) => card.cardNo)).toEqual(["BP05-070"])
  })

  it("never lists the card itself or a card without a representative printing", () => {
    const groups = relatedGroups({
      cardId: "c:base",
      related: related("c:base", "c:base", "c:unknown", "c:other"),
      mentions: [card("c:base")],
      representative,
      evolveKind,
    })
    expect(groups.evolve).toEqual([])
    expect(groups.mentions).toEqual([])
    expect(groups.otherOfficial.map((card) => card.cardNo)).toEqual(["BP05-070"])
  })
})
