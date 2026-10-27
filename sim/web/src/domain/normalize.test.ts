import { describe, expect, it } from "vitest"

import { compareCodePoints, normalizeText } from "./normalize"

describe("normalizeText", () => {
  it("folds width, case and separators so typed variants match", () => {
    expect(normalizeText("ＢＰ０１－００１")).toBe("bp01001")
    expect(normalizeText("Synthetic  Lantern")).toBe("syntheticlantern")
    expect(normalizeText("ﾌｧｹﾞ")).toBe("ファゲ".normalize("NFKC"))
    expect(normalizeText("  ")).toBe("")
  })
})

describe("compareCodePoints", () => {
  it("orders by code point, not UTF-16 unit", () => {
    expect(["𝔸", "ﬀ", "a"].sort(compareCodePoints)).toEqual(["a", "ﬀ", "𝔸"])
    expect(compareCodePoints("ab", "abc")).toBeLessThan(0)
    expect(compareCodePoints("x", "x")).toBe(0)
  })
})
