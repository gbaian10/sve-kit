import { describe, expect, it } from "vitest"

import { cardNoKey, cardNoLookupKey } from "./cardNo"
import { normalizeText } from "./normalize"

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
})

describe("card numbers", () => {
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
    expect(cardNoKey("xx01-001", sets)).toBeNull()
    expect(cardNoLookupKey("SD01-L01", sets)).toBe("sd01|l1|")
  })
})
