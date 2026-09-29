import { describe, expect, it } from "vitest"

import { cardNoKey, cardNoLookupKey } from "./cardNo"
import { normalizeText } from "./normalize"
import { DEFAULT_QUERY } from "./query/model"
import {
  apply,
  matchEntry,
  type SearchEntry,
  type SearchName,
  type SearchPrinting,
  suggest,
} from "./search"

const SETS = new Set(["bp01", "sd01", "pr"])
const name = (lang: SearchName["lang"], text: string): SearchName => ({
  lang,
  text,
  normalized: normalizeText(text),
})
const printing = (
  id: string,
  region: SearchPrinting["region"],
  cardNo: string,
  extra: Partial<SearchPrinting> = {},
): SearchPrinting => ({
  id,
  region,
  cardNo,
  lookupKey: cardNoLookupKey(cardNo, SETS),
  key: cardNoKey(cardNo, SETS),
  flat: normalizeText(cardNo),
  rarity: null,
  variant: "standard",
  artId: null,
  releasedOn: null,
  ...extra,
})
const entry = (
  over: Partial<SearchEntry> & Pick<SearchEntry, "cardId" | "order">,
): SearchEntry => ({
  classCode: "royal",
  setId: "set:bp01",
  names: [],
  aliases: [],
  printings: [],
  defaultPrinting: {},
  setCode: "bp01",
  faces: {},
  mechanic: () => "unknown",
  ...over,
})

const recruit = entry({
  cardId: "c:bp01-001",
  order: 1,
  names: [
    name("ja", "試作の見習い兵"),
    name("en", "Prototype Recruit"),
    name("zh-Hant", "試作見習兵"),
  ],
  printings: [
    printing("p:bp01-001", "jp", "BP01-001"),
    printing("p:bp01-001-en", "en", "BP01EN-001"),
  ],
  defaultPrinting: { jp: "p:bp01-001", en: "p:bp01-001-en" },
})
const templar = entry({
  cardId: "c:bp01-051",
  order: 51,
  classCode: "bishop",
  names: [name("ja", "試作の聖堂騎士"), name("en", "Prototype Templar")],
  aliases: [name("ja", "テンプラー")],
  printings: [
    printing("p:bp01-051", "jp", "BP01-051"),
    printing("p:bp01-051-en", "en", "BP01EN-051"),
  ],
  defaultPrinting: { jp: "p:bp01-051", en: "p:bp01-051-en" },
})
const promo = entry({
  cardId: "c:pr-001",
  order: 900,
  classCode: null,
  setId: "set:pr",
  names: [name("ja", "試作の妖精王")],
  printings: [printing("p:pr-001", "jp", "PR-001")],
  defaultPrinting: { jp: "p:pr-001" },
})
const ENTRIES = [recruit, templar, promo]
const options = { edition: "jp" as const, sets: SETS }

describe("matchEntry", () => {
  it.each(["bp01-51", "BP01 051", "BP01-051EN", "bp01051"])(
    "hits the card number for %s",
    (typed) => {
      expect(matchEntry(typed, templar, options)).toMatchObject({ field: "cardNo", rank: 0 })
      expect(matchEntry(typed, recruit, options)).toBeNull()
    },
  )

  it("opens the printing whose number was typed", () => {
    expect(matchEntry("BP01-051EN", templar, options)?.printingId).toBe("p:bp01-051-en")
    expect(matchEntry("bp01-51", templar, options)?.printingId).toBe("p:bp01-051")
  })

  it("ranks number prefix above name prefix above name contains", () => {
    expect(matchEntry("bp01-0", templar, options)).toMatchObject({ field: "cardNo", rank: 1 })
    expect(matchEntry("bp01-5", templar, options)).toMatchObject({ field: "cardNo", rank: 1 })
    expect(matchEntry("bp01", templar, options)).toMatchObject({ field: "cardNo", rank: 1 })
    expect(matchEntry("51", templar, options)).toMatchObject({ field: "cardNo", rank: 0 })
    expect(matchEntry("051", templar, options)).toMatchObject({ field: "cardNo", rank: 0 })
    expect(matchEntry("5", templar, options)).toMatchObject({ field: "cardNo", rank: 1 })
    expect(matchEntry("52", templar, options)).toBeNull()
    expect(matchEntry("bp01-6", templar, options)).toBeNull()
    expect(matchEntry("bp02-5", templar, options)).toBeNull()
    expect(matchEntry("試作の聖", templar, options)).toMatchObject({ field: "name", rank: 2 })
    expect(matchEntry("聖堂", templar, options)).toMatchObject({ field: "name", rank: 3 })
    expect(matchEntry("templ", templar, options)).toMatchObject({ field: "name", rank: 3 })
  })

  it("matches aliases and says so, preferring a name hit of the same strength", () => {
    expect(matchEntry("テンプラー", templar, options)).toMatchObject({ field: "alias", rank: 2 })
    expect(matchEntry("試作", templar, options)).toMatchObject({ field: "name", rank: 2 })
  })

  it("uses the edition's default printing for text hits", () => {
    expect(matchEntry("templar", templar, { ...options, edition: "en" })?.printingId).toBe(
      "p:bp01-051-en",
    )
    expect(matchEntry("妖精", promo, { ...options, edition: "en" })?.printingId).toBe("p:pr-001")
  })

  it("ignores empty or unknown text", () => {
    expect(matchEntry("   ", templar, options)).toBeNull()
    expect(matchEntry("zzz", templar, options)).toBeNull()
  })
})

describe("suggest", () => {
  it("orders by rank then canonical order and applies the limit", () => {
    expect(suggest("試作", ENTRIES, { ...options, limit: 10 }).map((item) => item.cardId)).toEqual([
      "c:bp01-001",
      "c:bp01-051",
      "c:pr-001",
    ])
    expect(suggest("bp01-51", ENTRIES, { ...options, limit: 10 })).toEqual([
      { cardId: "c:bp01-051", field: "cardNo", rank: 0, printingId: "p:bp01-051" },
    ])
    expect(suggest("試作", ENTRIES, { ...options, limit: 2 })).toHaveLength(2)
    expect(suggest("", ENTRIES, { ...options, limit: 2 })).toEqual([])
  })
})

describe("apply", () => {
  it("lists every card in canonical order when nothing is set", () => {
    expect(apply(DEFAULT_QUERY, ENTRIES, options)).toEqual([
      { key: "c:bp01-001", printingId: "p:bp01-001" },
      { key: "c:bp01-051", printingId: "p:bp01-051" },
      { key: "c:pr-001", printingId: "p:pr-001" },
    ])
  })

  it("filters by class with neutral standing for classless cards", () => {
    expect(apply({ ...DEFAULT_QUERY, classes: ["bishop"] }, ENTRIES, options)).toEqual([
      { key: "c:bp01-051", printingId: "p:bp01-051" },
    ])
    expect(apply({ ...DEFAULT_QUERY, classes: ["neutral", "royal"] }, ENTRIES, options)).toEqual([
      { key: "c:bp01-001", printingId: "p:bp01-001" },
      { key: "c:pr-001", printingId: "p:pr-001" },
    ])
  })

  it("puts a whole-number hit before a same-digits prefix across sets", () => {
    const later = entry({
      cardId: "c:sd01-510",
      order: 2,
      printings: [printing("p:sd01-510", "jp", "SD01-510")],
      defaultPrinting: { jp: "p:sd01-510" },
    })
    expect(
      apply({ ...DEFAULT_QUERY, text: "51" }, [later, templar], options).map((item) => item.key),
    ).toEqual(["c:bp01-051", "c:sd01-510"])
  })

  it("orders text hits like the suggest list: number hits before name hits", () => {
    // "試作の妖精王" contains 試作 (name, early in the snapshot); BP01-051 is a number hit later on.
    const late = entry({
      cardId: "c:sd01-005",
      order: 5,
      names: [name("ja", "妖精の051")],
      printings: [printing("p:sd01-005", "jp", "SD01-005")],
      defaultPrinting: { jp: "p:sd01-005" },
    })
    expect(
      apply({ ...DEFAULT_QUERY, text: "051" }, [late, templar], options).map((item) => item.key),
    ).toEqual(["c:bp01-051", "c:sd01-005"])
  })

  it("combines text and class and keeps the typed printing", () => {
    expect(
      apply({ ...DEFAULT_QUERY, text: "BP01EN", classes: ["bishop"] }, ENTRIES, options),
    ).toEqual([{ key: "c:bp01-051", printingId: "p:bp01-051-en" }])
    expect(apply({ ...DEFAULT_QUERY, text: "nothing" }, ENTRIES, options)).toEqual([])
  })
})

describe("apply facets, units and sorts", () => {
  const stats = (over: Partial<import("./search").FaceStats> = {}) => ({
    typeCode: "follower",
    cost: 2,
    attack: 2,
    defense: 2,
    name: "試作",
    ...over,
  })
  const a = entry({
    cardId: "c:a",
    order: 1,
    setCode: "bp01",
    faces: { jp: stats({ cost: 1, attack: 1, defense: 3, name: "い" }) },
    printings: [
      printing("p:a", "jp", "BP01-001", {
        rarity: "bronze",
        artId: "art:a",
        releasedOn: "2026-02-01",
      }),
      printing("p:a-alt", "jp", "BP01-001a", {
        rarity: "bronze",
        variant: "alt",
        artId: "art:a2",
        releasedOn: "2026-03-01",
      }),
    ],
    defaultPrinting: { jp: "p:a" },
    mechanic: (id) => (id === "kw:ward" ? "present" : "absent"),
  })
  const b = entry({
    cardId: "c:b",
    order: 2,
    setCode: "sd01",
    setId: "set:sd01",
    faces: { jp: stats({ cost: 8, attack: 5, defense: 5, name: "あ", typeCode: "spell" }) },
    printings: [
      printing("p:b", "jp", "SD01-001", {
        rarity: "gold",
        artId: "art:b",
        releasedOn: "2026-01-01",
      }),
      printing("p:b-sign", "jp", "SD01-001S", {
        rarity: "gold",
        variant: "signed",
        artId: "art:b",
        releasedOn: "2026-01-01",
      }),
    ],
    defaultPrinting: { jp: "p:b" },
    mechanic: () => "unknown",
  })
  const c = entry({
    cardId: "c:c",
    order: 3,
    faces: { jp: stats({ cost: null, attack: null, defense: null, name: "う" }) },
    printings: [printing("p:c", "jp", "BP01-003", { rarity: "silver", artId: "art:c" })],
    defaultPrinting: { jp: "p:c" },
    mechanic: () => "unknown",
  })
  const all = [a, b, c]
  const keys = (state: Partial<typeof DEFAULT_QUERY>) =>
    apply({ ...DEFAULT_QUERY, ...state }, all, options).map((item) => item.key)

  it("filters by set, type, cost (7 = 7+), rarity and alt art", () => {
    expect(keys({ sets: ["sd01"] })).toEqual(["c:b"])
    expect(keys({ types: ["spell"] })).toEqual(["c:b"])
    expect(keys({ cost: { min: 2 } })).toEqual(["c:b"])
    expect(keys({ cost: { max: 7 } })).toEqual(["c:a", "c:b"])
    expect(keys({ cost: { min: 7, max: 7 } })).toEqual(["c:b"])
    expect(keys({ rarities: ["gold", "silver"] })).toEqual(["c:b", "c:c"])
    expect(keys({ altArtOnly: true })).toEqual(["c:a", "c:b"])
    expect(apply({ ...DEFAULT_QUERY, altArtOnly: true }, all, options)[0]?.printingId).toBe(
      "p:a-alt",
    )
  })

  it("filters by mechanic tri-state: has needs present, not needs absent", () => {
    expect(keys({ mechanics: { "kw:ward": "has" } })).toEqual(["c:a"])
    expect(keys({ mechanics: { "kw:x": "not" } })).toEqual(["c:a"])
    expect(keys({ mechanics: { "kw:ward": "not" } })).toEqual([])
  })

  it("groups by unit: card, art, printing", () => {
    expect(apply({ ...DEFAULT_QUERY, unit: "art" }, all, options)).toEqual([
      { key: "art:a", printingId: "p:a" },
      { key: "art:a2", printingId: "p:a-alt" },
      { key: "art:b", printingId: "p:b" },
      { key: "art:c", printingId: "p:c" },
    ])
    expect(
      apply({ ...DEFAULT_QUERY, unit: "printing" }, all, options).map((item) => item.key),
    ).toEqual(["p:a", "p:a-alt", "p:b", "p:b-sign", "p:c"])
  })

  it("judges type, cost and mechanics per printing and lists the printing that passes", () => {
    const d = entry({
      cardId: "c:d",
      order: 4,
      faces: { jp: stats({ cost: 2 }), en: stats({ cost: 3, typeCode: "spell", name: "Proto" }) },
      printings: [
        printing("p:d", "jp", "BP01-004", { artId: "art:d" }),
        printing("p:d-en", "en", "BP01EN-004", { artId: "art:d" }),
      ],
      defaultPrinting: { jp: "p:d", en: "p:d-en" },
      mechanic: (id, region) => (id === "kw:x" && region === "en" ? "present" : "absent"),
    })
    const only = (state: Partial<typeof DEFAULT_QUERY>) =>
      apply({ ...DEFAULT_QUERY, ...state }, [d], options)
    expect(only({ cost: { min: 3, max: 3 } })).toEqual([{ key: "c:d", printingId: "p:d-en" }])
    expect(only({ types: ["spell"], unit: "art" })).toEqual([
      { key: "art:d", printingId: "p:d-en" },
    ])
    expect(only({ mechanics: { "kw:x": "has" }, unit: "printing" })).toEqual([
      { key: "p:d-en", printingId: "p:d-en" },
    ])
    expect(only({ cost: { min: 2, max: 2 } })).toEqual([{ key: "c:d", printingId: "p:d" }])
    expect(only({ cost: { min: 4 } })).toEqual([])
  })

  it("sorts by cost, attack, defense, name and date with unknowns last", () => {
    expect(keys({ sort: "cost" })).toEqual(["c:a", "c:b", "c:c"])
    expect(keys({ sort: "atk" })).toEqual(["c:a", "c:b", "c:c"])
    expect(keys({ sort: "def" })).toEqual(["c:a", "c:b", "c:c"])
    expect(keys({ sort: "name" })).toEqual(["c:b", "c:a", "c:c"])
    expect(keys({ sort: "date" })).toEqual(["c:b", "c:a", "c:c"])
  })
})
