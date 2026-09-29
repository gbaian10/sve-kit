import { describe, expect, it } from "vitest"

import { compareCodePoints, normalizeLoose, normalizeText } from "./normalize"

describe("normalizeText", () => {
  it("folds width, case and separators so typed variants match", () => {
    expect(normalizeText("ＢＰ０１－００１")).toBe("bp01001")
    expect(normalizeText("Fairy  Whisperer")).toBe("fairywhisperer")
    expect(normalizeText("ﾌｧｹﾞ")).toBe("ファゲ".normalize("NFKC"))
    expect(normalizeText("  ")).toBe("")
  })
  it("loose form keeps single spaces between words", () => {
    expect(normalizeLoose("  Fairy\u3000Whisperer - EX ")).toBe("fairy whisperer ex")
  })
})

describe("compareCodePoints", () => {
  it("orders by code point, not UTF-16 unit", () => {
    expect(["𝔸", "ﬀ", "a"].sort(compareCodePoints)).toEqual(["a", "ﬀ", "𝔸"])
    expect(compareCodePoints("ab", "abc")).toBeLessThan(0)
    expect(compareCodePoints("x", "x")).toBe(0)
  })
})
