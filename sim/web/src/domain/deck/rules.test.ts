import { describe, expect, it } from "vitest"

import { card, lookup } from "../../test-utils/deck-cards"
import { createDeck } from "./model"
import { checkDeck, type DeckProblem } from "./rules"
import type { Deck, DeckBasis, DeckEntry, DeckRestriction } from "./types"

const NOW = new Date("2026-11-03T00:00:00.000Z")
const forest: DeckBasis = { kind: "class", classCode: "forest" }

const cards = lookup(
  card("leader", { typeCode: "leader", specialKinds: [], classCode: "forest" }),
  card("leader-sword", { typeCode: "leader", classCode: "sword" }),
  card("elf"),
  card("elf-alt", { name: "elf" }),
  card("neutral", { classCode: "neutral" }),
  card("sword", { classCode: "sword" }),
  card("evo", { specialKinds: ["evolve"], cost: null }),
  card("token", { specialKinds: ["token"] }),
  card("uma", { titles: ["umamusume"] }),
)

function deck(change: Partial<Deck> = {}): Deck {
  return { ...createDeck({ name: "D", edition: "jp", basis: forest }, NOW), ...change }
}

/** Forty legal main-deck copies, built from distinct synthetic names. */
const fullMain: DeckEntry[] = Array.from({ length: 14 }, (_, i) => ({
  cardId: `filler-${String(i)}`,
  count: i < 12 ? 3 : 2,
}))
const fillers = Array.from({ length: 14 }, (_, i) => card(`filler-${String(i)}`))
const withFillers = (cardId: string) =>
  cards(cardId) ?? fillers.find((item) => item.cardId === cardId)

const kinds = (target: Deck, restrictions = new Map<string, DeckRestriction[]>()) =>
  checkDeck(target, { infoOf: withFillers, restrictions }).map((item) => item.kind)

describe("checkDeck", () => {
  it("finds nothing wrong with a legal forty-card deck and its leader", () => {
    expect(kinds(deck({ leader: { cardId: "leader" }, main: fullMain }))).toEqual([])
  })

  it("warns about a missing leader and lists errors before warnings", () => {
    const found = kinds(deck({ main: fullMain.slice(1) }))
    expect(found).toEqual(["mainTooFew", "leaderMissing"] satisfies DeckProblem["kind"][])
  })

  it("checks deck sizes", () => {
    const big = [
      ...fullMain,
      ...Array.from({ length: 4 }, (_, i) => ({ cardId: `filler-${String(i + 20)}`, count: 3 })),
    ]
    expect(kinds(deck({ leader: { cardId: "leader" }, main: big }))).toContain("mainTooMany")
    expect(
      kinds(
        deck({
          leader: { cardId: "leader" },
          main: fullMain,
          evolve: [{ cardId: "evo", count: 11 }],
        }),
      ),
    ).toContain("sameNameOver")
    expect(
      kinds(
        deck({
          leader: { cardId: "leader" },
          main: fullMain,
          evolve: [{ cardId: "evo", count: 11 }],
        }),
      ),
    ).toContain("evolveTooMany")
  })

  it("counts copies per card name, across card ids", () => {
    const main = [
      ...fullMain.slice(1),
      { cardId: "elf", count: 2 },
      { cardId: "elf-alt", count: 2 },
    ]
    const problems = checkDeck(deck({ leader: { cardId: "leader" }, main }), {
      infoOf: withFillers,
      restrictions: new Map(),
    })
    expect(problems.find((item) => item.kind === "sameNameOver")).toMatchObject({
      cardIds: ["elf", "elf-alt"],
      values: { name: "elf", count: 4, limit: 3, section: "main" },
    })
  })

  it("keeps cards in the deck they belong to and tokens out", () => {
    const found = kinds(
      deck({
        leader: { cardId: "leader" },
        main: [...fullMain, { cardId: "evo", count: 1 }, { cardId: "token", count: 1 }],
        evolve: [{ cardId: "elf", count: 1 }],
      }),
    )
    expect(found).toEqual(
      expect.arrayContaining(["mainWrongCard", "tokenInDeck", "evolveWrongCard"]),
    )
  })

  it("allows the class and neutral cards on a class basis, neutral only on a neutral basis", () => {
    const main = [
      ...fullMain.slice(2),
      { cardId: "neutral", count: 1 },
      { cardId: "sword", count: 1 },
    ]
    expect(kinds(deck({ leader: { cardId: "leader" }, main }))).toEqual([
      "mainTooFew",
      "classMismatch",
    ])
    expect(kinds(deck({ leader: { cardId: "leader-sword" }, main: fullMain }))).toEqual([
      "leaderClassMismatch",
    ])
    const neutral = deck({
      basis: { kind: "class", classCode: "neutral" },
      leader: { cardId: "leader-sword" },
      main: [{ cardId: "elf", count: 1 }],
    })
    expect(kinds(neutral)).toEqual(expect.arrayContaining(["classMismatchNeutral"]))
    expect(kinds(neutral)).not.toContain("leaderClassMismatch")
  })

  it("needs the title on the leader and every card of a title deck", () => {
    const found = kinds(
      deck({
        basis: { kind: "title", titleCode: "umamusume" },
        leader: { cardId: "leader" },
        main: [
          { cardId: "uma", count: 1 },
          { cardId: "elf", count: 1 },
        ],
      }),
    )
    expect(found).toEqual(expect.arrayContaining(["leaderTitleMismatch", "titleMismatch"]))
    expect(found.filter((kind) => kind === "titleMismatch")).toHaveLength(1)
  })

  it("notes titles whose own rules are not checked", () => {
    const vanguard = deck({ basis: { kind: "title", titleCode: "cardfight_vanguard" } })
    expect(kinds(vanguard)).toContain("titleRulesUnchecked")
  })

  it("reports restricted and banned cards by the copies under one restriction", () => {
    const restrictions = new Map<string, DeckRestriction[]>([
      ["elf", [{ id: "r-elf", maxCopies: 1 }]],
      ["elf-alt", [{ id: "r-elf", maxCopies: 1 }]],
      ["neutral", [{ id: "r-ban", maxCopies: 0 }]],
      ["filler-0", [{ id: "r-open", maxCopies: null }]],
    ])
    const main = [
      ...fullMain,
      { cardId: "elf", count: 1 },
      { cardId: "elf-alt", count: 1 },
      { cardId: "neutral", count: 1 },
    ]
    const found = kinds(deck({ leader: { cardId: "leader" }, main }), restrictions)
    expect(found).toEqual(expect.arrayContaining(["restricted", "banned"]))
    expect(found.filter((kind) => kind === "restricted")).toHaveLength(1)
  })

  it("warns about cards the snapshot does not know instead of guessing", () => {
    const found = kinds(
      deck({ leader: { cardId: "gone" }, main: [...fullMain, { cardId: "missing", count: 1 }] }),
    )
    expect(found.filter((kind) => kind === "unknownCard")).toHaveLength(2)
  })

  it("never checks the side list", () => {
    const side = [
      { cardId: "sword", count: 9 },
      { cardId: "token", count: 1 },
    ]
    expect(kinds(deck({ leader: { cardId: "leader" }, main: fullMain, side }))).toEqual([])
  })
})
