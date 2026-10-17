import { expect, it } from "vitest"

import { setLabel } from "./sets"

it("selects a family label by interface language and falls back to its original", () => {
  const original = { text: "試作パック", lang: "ja" as const }
  const name = { original, translations: { "zh-Hant": "試作卡包" } }
  expect(setLabel(name, "zh-Hant")).toEqual({ text: "試作卡包", lang: "zh-Hant" })
  expect(setLabel(name, "ja")).toEqual(original)
  expect(setLabel(name, "en")).toEqual(original)
  expect(setLabel({ original, translations: {} }, "zh-Hant")).toEqual(original)
  expect(setLabel({ original, translations: { "zh-Hant": "" } }, "zh-Hant")).toEqual(original)
})
