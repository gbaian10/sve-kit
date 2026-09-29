import { describe, expect, it } from "vitest"

import { resolveFaceText, type TranslationCandidate } from "./textLanguage"

const ja = { lang: "ja" as const, text: "【守護】" }
const en = { lang: "en" as const, text: "Ward" }
const zh: TranslationCandidate = {
  lang: "zh-Hant",
  text: "【守護】",
  origin: "project",
  status: "reviewed",
  basis: "shared_jp",
}
const enOfficial: TranslationCandidate = {
  lang: "en",
  text: "Ward",
  origin: "official_sve",
  status: "reviewed",
  basis: "official_counterpart",
}
const enMachine: TranslationCandidate = {
  lang: "en",
  text: "Guard",
  origin: "machine",
  status: "draft",
  basis: "own_source",
}
const jaOfEn: TranslationCandidate = {
  lang: "ja",
  text: "【守護】",
  origin: "official_sve",
  status: "reviewed",
  basis: "official_counterpart",
}

describe("resolveFaceText (3×2 matrix)", () => {
  it("zh-TW UI: original plus Traditional Chinese, or a missing notice", () => {
    expect(
      resolveFaceText({
        edition: "jp",
        uiLanguage: "zh-TW",
        original: ja,
        fallback: en,
        translations: [zh, enOfficial],
      }),
    ).toEqual({
      original: ja,
      translation: { lang: "zh-Hant", text: "【守護】", label: "project" },
      notices: [],
    })
    expect(
      resolveFaceText({
        edition: "jp",
        uiLanguage: "zh-TW",
        original: ja,
        fallback: en,
        translations: [enOfficial],
      }),
    ).toEqual({
      original: ja,
      translation: null,
      notices: ["missing"],
    })
    expect(
      resolveFaceText({
        edition: "en",
        uiLanguage: "zh-TW",
        original: en,
        fallback: ja,
        translations: [zh],
      })?.translation?.text,
    ).toBe("【守護】")
    expect(
      resolveFaceText({
        edition: "en",
        uiLanguage: "zh-TW",
        original: en,
        fallback: ja,
        translations: [],
      })?.notices,
    ).toEqual(["missing"])
  })

  it("ja UI: Japanese alone, or English plus its Japanese counterpart", () => {
    expect(
      resolveFaceText({
        edition: "jp",
        uiLanguage: "ja",
        original: ja,
        fallback: en,
        translations: [zh],
      }),
    ).toEqual({ original: ja, translation: null, notices: [] })
    expect(
      resolveFaceText({
        edition: "en",
        uiLanguage: "ja",
        original: en,
        fallback: ja,
        translations: [jaOfEn],
      })?.translation,
    ).toEqual({ lang: "ja", text: "【守護】", label: "official" })
    expect(
      resolveFaceText({
        edition: "en",
        uiLanguage: "ja",
        original: en,
        fallback: ja,
        translations: [zh],
      }),
    ).toEqual({ original: en, translation: null, notices: ["missing"] })
  })

  it("en UI: Japanese plus official English, reviewed before machine, never Chinese", () => {
    expect(
      resolveFaceText({
        edition: "jp",
        uiLanguage: "en",
        original: ja,
        fallback: en,
        translations: [zh, enMachine, enOfficial],
      })?.translation,
    ).toEqual({ lang: "en", text: "Ward", label: "official" })
    expect(
      resolveFaceText({
        edition: "jp",
        uiLanguage: "en",
        original: ja,
        fallback: en,
        translations: [zh, enMachine],
      })?.translation,
    ).toEqual({ lang: "en", text: "Guard", label: "machine" })
    expect(
      resolveFaceText({
        edition: "jp",
        uiLanguage: "en",
        original: ja,
        fallback: en,
        translations: [zh],
      }),
    ).toEqual({ original: ja, translation: null, notices: ["missing"] })
    expect(
      resolveFaceText({
        edition: "en",
        uiLanguage: "en",
        original: en,
        fallback: ja,
        translations: [zh],
      }),
    ).toEqual({ original: en, translation: null, notices: [] })
  })

  it("falls back to the other region with a notice when the edition is not released", () => {
    expect(
      resolveFaceText({
        edition: "en",
        uiLanguage: "zh-TW",
        original: undefined,
        fallback: ja,
        translations: [zh],
      }),
    ).toEqual({
      original: ja,
      translation: { lang: "zh-Hant", text: "【守護】", label: "project" },
      notices: ["no_edition"],
    })
    expect(
      resolveFaceText({
        edition: "en",
        uiLanguage: "en",
        original: undefined,
        fallback: ja,
        translations: [],
      })?.notices,
    ).toEqual(["no_edition", "missing"])
    expect(
      resolveFaceText({
        edition: "jp",
        uiLanguage: "ja",
        original: undefined,
        fallback: undefined,
        translations: [],
      }),
    ).toBeNull()
  })
})
