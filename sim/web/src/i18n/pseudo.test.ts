import { describe, expect, it } from "vitest"

import { createI18n } from "./index"
import { pseudoLocalize } from "./pseudo"

describe("pseudoLocalize", () => {
  it("accents Latin letters, keeps digits and CJK, and grows the text by at least 35%", () => {
    const out = pseudoLocalize("Card names 12")
    expect(out.startsWith("[")).toBe(true)
    expect(out.endsWith("]")).toBe(true)
    expect(out).toContain("Çáŕð ñáḿéš 12")
    expect(Array.from(out).length).toBeGreaterThanOrEqual(Math.ceil(13 * 1.35) + 2)
    expect(pseudoLocalize("卡名顯示")).toBe("[卡名顯示 〜〜]")
    expect(pseudoLocalize("a".repeat(20))).toBe("[áááááááááááááááááááá ~~~ ~~~ ~]")
  })

  it("applies to every translation when enabled", async () => {
    const i18n = await createI18n("en", { pseudo: true })
    expect(i18n.t("nav.home")).toBe("[Ĥóḿé ~~]")
    expect(i18n.t("footer.dataVersion", { version: "42" })).toContain("42")
    const plain = await createI18n("en")
    expect(plain.t("nav.home")).toBe("Home")
  })
})
