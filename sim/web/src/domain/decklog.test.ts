import { describe, expect, it } from "vitest"

import { deckLogReferenceOf, normalizeDeckLogCode, readDeckLogCards } from "./decklog"

describe("deckLogReferenceOf", () => {
  it("reads a DeckLog link's edition and code", () => {
    expect(deckLogReferenceOf("https://decklog.bushiroad.com/view/6LPM7")).toEqual({
      region: "jp",
      code: "6LPM7",
    })
    expect(deckLogReferenceOf("  decklog-en.bushiroad.com/view/15dbx/?x=1 ")).toEqual({
      region: "en",
      code: "15DBX",
    })
    expect(deckLogReferenceOf("https://decklog.bushiroad.com/copy/1AMWVS#top")).toEqual({
      region: "jp",
      code: "1AMWVS",
    })
  })

  it("leaves a bare code's edition open", () => {
    expect(deckLogReferenceOf("vtbp")).toEqual({ region: undefined, code: "VTBP" })
  })

  it("is not fooled by lists, share codes or other sites", () => {
    for (const text of [
      "3 BP01-001",
      "SVEK-AQAB",
      "ABC",
      "ABCDEFGHIJK",
      "https://decklog.bushiroad.com.example/view/6LPM7",
      "https://example.com/view/6LPM7",
      "https://decklog.bushiroad.com/view/6LPM7/extra",
    ])
      expect(deckLogReferenceOf(text), text).toBeUndefined()
    expect(normalizeDeckLogCode("6lpm7")).toBe("6LPM7")
  })
})

describe("readDeckLogCards", () => {
  it("reads DeckLog's rows and ours, and an absent section as empty", () => {
    expect(readDeckLogCards([{ card_number: "BP09-R047EN", num: 3, name: "x" }])).toEqual([
      { cardNo: "BP09-R047EN", count: 3 },
    ])
    expect(readDeckLogCards([{ cardNo: "PR-339", count: 1 }])).toEqual([
      { cardNo: "PR-339", count: 1 },
    ])
    expect(readDeckLogCards(null)).toEqual([])
  })

  it("rejects the whole section when any row is malformed", () => {
    for (const rows of [
      [{ card_number: "BP01-001", num: 1.5 }],
      [{ card_number: "BP01 001", num: 1 }],
      [null],
      {},
      Array.from({ length: 121 }, () => ({ card_number: "BP01-001", num: 1 })),
    ])
      expect(readDeckLogCards(rows)).toBeUndefined()
  })
})
