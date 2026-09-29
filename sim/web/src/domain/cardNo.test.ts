import { describe, expect, it } from "vitest"

import { cardNoKey, cardNoLookupKey, cardNoMatches, cardNoPrefixMatches } from "./cardNo"
import { normalizeLoose, normalizeText } from "./normalize"

describe("normalizeText", () => {
  it.each([
    ["ＢＰ０１－０５１", "bp01051"],
    ["Bp01 051", "bp01051"],
    ["試作　の　剣士", "試作の剣士"],
    ["ｶﾞﾝﾀﾞﾑ", "ガンダム"],
    ["Prototype—Recruit", "prototyperecruit"],
    ["", ""],
  ])("folds %j to %j", (input, expected) => {
    expect(normalizeText(input)).toBe(expected)
  })

  it("keeps single spaces in the loose form", () => {
    expect(normalizeLoose("  Prototype   Recruit ")).toBe("prototype recruit")
  })
})

describe("card numbers", () => {
  it.each([
    ["bp01-51", "BP01-051"],
    ["BP01 051", "BP01-051"],
    ["BP01-051EN", "BP01EN-051"],
    ["bp01en-51", "BP01EN-051"],
    ["BP01EN051", "BP01-051EN"],
    ["SD01-L01EN", "SD01EN-L01"],
    ["ｂｐ０１－０５１", "BP01-051"],
    ["sd01-l1", "SD01-L01"],
    ["bp01-2a", "BP01-002a"],
    ["pr-1", "PR-001"],
  ])("%j matches %j", (typed, printed) => {
    expect(cardNoMatches(typed, printed)).toBe(true)
  })

  it.each([
    ["BP01-051", "BP01EN-051"],
    ["BP01-051EN", "BP01-051"],
    ["BP01EN-051", "BP01-051"],
    ["BP01-051", "BP02-051"],
    ["BP01-051", "BP01-052"],
    ["bp01-002", "BP01-002a"],
    ["試作", "BP01-051"],
  ])("%j does not match %j", (typed, printed) => {
    expect(cardNoMatches(typed, printed)).toBe(false)
  })

  it("exposes a lookup key and rejects text that is not a number", () => {
    expect(cardNoKey("BP01-051EN")).toEqual({ set: "bp01", number: "51", suffix: "en" })
    expect(cardNoLookupKey("BP01 051")).toBe("bp01|51|")
    expect(cardNoKey("試作の剣士")).toBeNull()
    expect(cardNoKey("")).toBeNull()
  })

  it("uses the snapshot's set codes when given, so digit-less and digit-bearing codes both split", () => {
    const sets = new Set(["bp01", "pr", "csd02a", "sd01"])
    expect(cardNoKey("PR-001", sets)).toEqual({ set: "pr", number: "1", suffix: "" })
    expect(cardNoKey("csd02a-001", sets)).toEqual({ set: "csd02a", number: "1", suffix: "" })
    expect(cardNoMatches("bp01 51", "BP01-051", sets)).toBe(true)
    expect(cardNoKey("xx01-001", sets)).toBeNull()
    expect(cardNoLookupKey("SD01-L01", sets)).toBe("sd01|l1|")
  })

  it("matches prefixes for suggestions", () => {
    expect(cardNoPrefixMatches("bp01", "BP01-051")).toBe(true)
    expect(cardNoPrefixMatches("BP01-05", "BP01-051")).toBe(true)
    expect(cardNoPrefixMatches("bp02", "BP01-051")).toBe(false)
    expect(cardNoPrefixMatches("", "BP01-051")).toBe(false)
  })
})
