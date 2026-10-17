import { expect, it } from "vitest"

import { setCaption, setLabel } from "./sets"

it("selects a family label by interface language and falls back to its original", () => {
  const original = { text: "試作パック", lang: "ja" as const }
  const name = { original, translations: { "zh-Hant": "試作卡包" } }
  expect(setLabel(name, "zh-Hant")).toEqual({ text: "試作卡包", lang: "zh-Hant" })
  expect(setLabel(name, "ja")).toEqual(original)
  expect(setLabel(name, "en")).toEqual(original)
  expect(setLabel({ original, translations: {} }, "zh-Hant")).toEqual(original)
  expect(setLabel({ original, translations: { "zh-Hant": "" } }, "zh-Hant")).toEqual(original)
})

it("drops the code from a caption whose name already starts with it", () => {
  const original = { text: "試作プロモ", lang: "ja" as const }
  const promo = {
    code: "pr",
    publicCode: "PR",
    name: { original, translations: { "zh-Hant": "PR卡", en: "Promo Cards" } },
  }
  expect(setCaption(promo, "zh-Hant")).toEqual({
    code: null,
    name: { text: "PR卡", lang: "zh-Hant" },
    text: "PR卡",
  })
  expect(setCaption(promo, "en")).toEqual({
    code: "PR",
    name: { text: "Promo Cards", lang: "en" },
    text: "PR Promo Cards",
  })
  expect(setCaption(promo, "ja")).toEqual({ code: "PR", name: original, text: "PR 試作プロモ" })
  // Without a public code the family code stands in, and the same rule applies to it.
  const bare = {
    code: "bp01",
    name: { original: { text: "bp01 pack", lang: "en" as const }, translations: {} },
  }
  expect(setCaption(bare, "zh-Hant").text).toBe("bp01 pack")
})
