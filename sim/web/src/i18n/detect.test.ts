import { describe, expect, it } from "vitest"

import { detectUiLanguage, preferredLanguages } from "./detect"

describe("detectUiLanguage", () => {
  it.each([
    [["zh-TW"], "zh-TW"],
    [["zh-Hant-HK"], "zh-TW"],
    [["zh-CN"], "zh-TW"],
    [["zh-Hans"], "zh-TW"],
    [["zh"], "zh-TW"],
    [["ZH_tw"], "zh-TW"],
    [["ja"], "ja"],
    [["ja-JP"], "ja"],
    [["en"], "en"],
    [["en-GB"], "en"],
    [["ko-KR"], "ja"],
    [["fr", "de"], "ja"],
    [[], "ja"],
  ] as const)("%j -> %s", (preferred, expected) => {
    expect(detectUiLanguage(preferred)).toBe(expected)
  })

  it("uses the first supported language in preference order", () => {
    expect(detectUiLanguage(["ko", "en-US", "ja"])).toBe("en")
    expect(detectUiLanguage(["fr", "zh-CN", "en"])).toBe("zh-TW")
  })

  it("does not treat languages sharing a prefix as supported", () => {
    expect(detectUiLanguage(["jam", "eng", "en"])).toBe("en")
  })
})

describe("preferredLanguages", () => {
  it("prefers navigator.languages", () => {
    expect(preferredLanguages({ languages: ["en-US", "ja"], language: "fr" })).toEqual([
      "en-US",
      "ja",
    ])
  })

  it("falls back to navigator.language when languages is missing or empty", () => {
    expect(preferredLanguages({ language: "zh-CN" })).toEqual(["zh-CN"])
    expect(preferredLanguages({ languages: [], language: "en" })).toEqual(["en"])
  })

  it("returns no preference when neither is available", () => {
    expect(preferredLanguages({})).toEqual([])
    expect(preferredLanguages({ language: "" })).toEqual([])
    expect(preferredLanguages(undefined)).toEqual([])
  })

  it("ends at the Japanese default without any browser language", () => {
    expect(detectUiLanguage(preferredLanguages(undefined))).toBe("ja")
  })
})
