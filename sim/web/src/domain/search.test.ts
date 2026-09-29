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
): SearchPrinting => ({
  id,
  region,
  cardNo,
  lookupKey: cardNoLookupKey(cardNo, SETS),
  key: cardNoKey(cardNo, SETS),
  flat: normalizeText(cardNo),
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
