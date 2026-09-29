import { describe, expect, it } from "vitest"

import { displayName, type NameSource } from "./nameDisplay"

const jp: NameSource = {
  original: { lang: "ja", text: "試作の妖精" },
  translations: { "zh-Hant": "試作妖精", en: "Prototype Fairy" },
}
const untranslated: NameSource = {
  original: { lang: "ja", text: "試作の魔女見習い" },
  translations: {},
}

describe("displayName", () => {
  it("leads with the translation, the original, or both", () => {
    expect(displayName(jp, "zh-TW", "translated")).toEqual({
      primary: { text: "試作妖精", lang: "zh-Hant" },
      missingTranslation: false,
    })
    expect(displayName(jp, "zh-TW", "original")).toEqual({
      primary: { text: "試作の妖精", lang: "ja" },
      missingTranslation: false,
    })
    expect(displayName(jp, "zh-TW", "both")).toEqual({
      primary: { text: "試作の妖精", lang: "ja" },
      secondary: { text: "試作妖精", lang: "zh-Hant" },
      missingTranslation: false,
    })
  })

  it("shows only the original when it already is the UI language", () => {
    expect(displayName(jp, "ja", "both")).toEqual({
      primary: { text: "試作の妖精", lang: "ja" },
      missingTranslation: false,
    })
    expect(
      displayName({ ...jp, original: { lang: "en", text: "Prototype Fairy" } }, "en", "translated"),
    ).toEqual({ primary: { text: "Prototype Fairy", lang: "en" }, missingTranslation: false })
  })

  it("falls back to the original and flags a missing translation", () => {
    expect(displayName(untranslated, "zh-TW", "translated")).toEqual({
      primary: { text: "試作の魔女見習い", lang: "ja" },
      missingTranslation: true,
    })
    expect(displayName(untranslated, "en", "both")).toEqual({
      primary: { text: "試作の魔女見習い", lang: "ja" },
      missingTranslation: true,
    })
  })
})
