import { describe, expect, it } from "vitest"

import { card } from "../../test-utils/deck-cards"
import { DeckCodeError, encodeDeckCode } from "./code"
import { importEdition, type ImportLookup, readDeckLogImport, readImport } from "./import"

// Two printings of c:1 (default p:1, alt p:1a); a leader, an evolve card and a royal card.
const cards = {
  "c:1": { ...card("c:1"), defaultPrintingId: "p:1" },
  "c:evo": {
    ...card("c:evo", { specialKinds: ["evolve"], cost: null }),
    defaultPrintingId: "p:evo",
  },
  "c:leader": { ...card("c:leader", { typeCode: "leader" }), defaultPrintingId: "p:leader" },
  "c:royal": { ...card("c:royal", { classCode: "royal" }), defaultPrintingId: "p:royal" },
}
const printings: Record<string, { cardId: string; printingId: string; no: string; intId: number }> =
  {
    "p:1": { cardId: "c:1", printingId: "p:1", no: "BP01-001", intId: 20_001 },
    "p:1a": { cardId: "c:1", printingId: "p:1a", no: "BP01-001P", intId: 20_901 },
    "p:evo": { cardId: "c:evo", printingId: "p:evo", no: "BP01-010", intId: 20_010 },
    "p:leader": { cardId: "c:leader", printingId: "p:leader", no: "BP01-LD01", intId: 20_100 },
    "p:royal": { cardId: "c:royal", printingId: "p:royal", no: "BP01-020", intId: 20_020 },
  }
const lookup: ImportLookup = {
  printingByCardNo: (no) => Object.values(printings).find((item) => item.no === no.toUpperCase()),
  printingByIntId: (intId) => Object.values(printings).find((item) => item.intId === intId),
  infoOf: (cardId) => cards[cardId as keyof typeof cards],
}

describe("readImport", () => {
  it("places pasted cards where they belong, keeping only the side list from the headings", () => {
    const imported = readImport(
      [
        "# 精靈快攻",
        "## 主牌組",
        "3 BP01-001",
        "1 BP01-010 (an evolve card under the main heading)",
        "1 bp01-ld01",
        "## 進化牌組",
        "1 BP01-001P",
        "## 候補",
        "2 BP01-020",
      ].join("\n"),
      lookup,
    )
    expect(imported).toMatchObject({
      name: "精靈快攻",
      edition: undefined,
      // A pasted list names no format; the importer makes it a Standard deck.
      format: undefined,
      basis: { kind: "class", classCode: "forest" },
      leader: { cardId: "c:leader" },
      main: [{ cardId: "c:1", count: 4 }],
      evolve: [{ cardId: "c:evo", count: 1 }],
      side: [{ cardId: "c:royal", count: 2 }],
      unknown: [],
    })
  })

  it("keeps a chosen printing that is not the card's default", () => {
    const imported = readImport("2 BP01-001P", lookup)
    expect(imported.main).toEqual([{ cardId: "c:1", count: 2, printingId: "p:1a" }])
  })

  it("lists card numbers and lines it cannot use", () => {
    const imported = readImport("3 BP99-001\nhello\n2 BP01-001", lookup)
    expect(imported.unknown).toEqual([{ label: "BP99-001", count: 3 }])
    expect(imported.unrecognized).toEqual([{ line: 2, text: "hello" }])
    expect(imported.main).toEqual([{ cardId: "c:1", count: 2 }])
  })

  it("reads a share code with its edition, format, basis, leader, side list and unknown ids", () => {
    const code = encodeDeckCode({
      edition: "en",
      format: "gloryfinder",
      basis: { kind: "title", titleCode: "umamusume" },
      leader: 20_100,
      main: [
        { intId: 20_001, count: 3 },
        { intId: 29_999, count: 2 },
      ],
      evolve: [{ intId: 20_010, count: 2 }],
      side: [{ intId: 20_020, count: 1 }],
      name: "Uma",
    })
    expect(importEdition(`https://example.test/decks/import?d=${code}`)).toBe("en")
    expect(readImport(code, lookup)).toMatchObject({
      name: "Uma",
      edition: "en",
      format: "gloryfinder",
      basis: { kind: "title", titleCode: "umamusume" },
      leader: { cardId: "c:leader" },
      main: [{ cardId: "c:1", count: 3 }],
      evolve: [{ cardId: "c:evo", count: 2 }],
      side: [{ cardId: "c:royal", count: 1 }],
      unknown: [{ label: "#29999", count: 2 }],
    })
  })

  it("reads a code without a leader or a name", () => {
    const code = encodeDeckCode({
      edition: "jp",
      format: "standard",
      basis: { kind: "class", classCode: "royal" },
      leader: null,
      main: [{ intId: 20_020, count: 3 }],
      evolve: [],
      side: [],
      name: "",
    })
    expect(readImport(code, lookup)).toMatchObject({ name: undefined, leader: null, side: [] })
  })

  it("infers neutral when a list names no class", () => {
    expect(readImport("1 BP99-001", lookup).basis).toEqual({ kind: "class", classCode: "neutral" })
  })

  it("refuses a malformed code instead of guessing", () => {
    expect(() => readImport("SVEK-AQ", lookup)).toThrow(DeckCodeError)
    expect(importEdition("3 BP01-001")).toBeUndefined()
  })

  it("keeps the printing with the most copies when a card is listed under several", () => {
    expect(readImport("1 BP01-001\n2 BP01-001P", lookup).main).toEqual([
      { cardId: "c:1", count: 3, printingId: "p:1a" },
    ])
    // A tie keeps the first printing listed; the default printing is not written on the entry.
    expect(readImport("2 BP01-001\n2 BP01-001P", lookup).main).toEqual([
      { cardId: "c:1", count: 4 },
    ])
  })
})

describe("readDeckLogImport", () => {
  const deckLog = {
    region: "jp",
    code: "AB12C",
    format: "N",
    className: "エルフ",
    leader: [{ cardNo: "BP01-LD01", count: 1 }],
    main: [
      { cardNo: "BP01-001", count: 1 },
      { cardNo: "BP01-001P", count: 2 },
      { cardNo: "BP99-001", count: 3 },
    ],
    evolve: [{ cardNo: "BP01-010", count: 2 }],
  } as const

  it("reads DeckLog's printings into the edition's cards and lists the ones it cannot match", () => {
    expect(readDeckLogImport(deckLog, lookup)).toEqual({
      name: undefined,
      edition: "jp",
      format: "standard",
      // DeckLog's declared class, not one inferred from the cards.
      basis: { kind: "class", classCode: "elf" },
      leader: { cardId: "c:leader" },
      main: [{ cardId: "c:1", count: 3, printingId: "p:1a" }],
      evolve: [{ cardId: "c:evo", count: 2 }],
      side: [],
      unknown: [{ label: "BP99-001", count: 3 }],
      extraLeaders: [],
      unrecognized: [],
    })
  })

  it("keeps a declared title, and infers the basis when DeckLog's choice is unknown", () => {
    const basis = (className: string | null) =>
      readDeckLogImport({ ...deckLog, className }, lookup).basis
    expect(basis("Umamusume: Pretty Derby")).toEqual({
      kind: "title",
      titleCode: "umamusume_pretty_derby",
    })
    expect(basis("グランブルーファンタジー")).toEqual({
      kind: "title",
      titleCode: "granblue_fantasy",
    })
    expect(basis("Class")).toEqual({ kind: "class", classCode: "forest" })
    expect(basis(null)).toEqual({ kind: "class", classCode: "forest" })
  })

  it("lists a second leader instead of dropping it silently", () => {
    const imported = readDeckLogImport(
      {
        ...deckLog,
        leader: [
          { cardNo: "BP01-LD01", count: 1 },
          { cardNo: "BP01-LD01", count: 1 },
        ],
      },
      lookup,
    )
    expect(imported.leader).toEqual({ cardId: "c:leader" })
    expect(imported.extraLeaders).toEqual([{ label: "BP01-LD01", count: 1 }])
  })

  it("maps DeckLog's construction codes per edition and leaves unknown ones Standard", () => {
    const format = (region: "jp" | "en", code: string | null) =>
      readDeckLogImport({ ...deckLog, region, format: code }, lookup).format
    expect(format("jp", "X")).toBe("crossover")
    expect(format("en", "G")).toBe("gloryfinder")
    expect(format("en", "X")).toBeUndefined()
    expect(format("jp", null)).toBeUndefined()
  })
})
