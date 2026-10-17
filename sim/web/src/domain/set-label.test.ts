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
    code: "xp",
    publicCode: "XP",
    name: { original, translations: { "zh-Hant": "XP試作卡", en: "Prototype Extras" } },
  }
  expect(setCaption(promo, "zh-Hant")).toEqual({
    code: null,
    name: { text: "XP試作卡", lang: "zh-Hant" },
    text: "XP試作卡",
  })
  expect(setCaption(promo, "en")).toEqual({
    code: "XP",
    name: { text: "Prototype Extras", lang: "en" },
    text: "XP Prototype Extras",
  })
  expect(setCaption(promo, "ja")).toEqual({ code: "XP", name: original, text: "XP 試作プロモ" })
  // Without a public code the family code stands in, and the same rule applies to it.
  const bare = {
    code: "bp01",
    name: { original: { text: "bp01 pack", lang: "en" as const }, translations: {} },
  }
  expect(setCaption(bare, "zh-Hant").text).toBe("bp01 pack")
})
