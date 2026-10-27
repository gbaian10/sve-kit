import { describe, expect, it } from "vitest"

import { createI18n } from "./index"

describe("pseudoLocalize", () => {
  it("applies to every translation when enabled", async () => {
    const i18n = await createI18n("en", { pseudo: true })
    expect(i18n.t("nav.home")).toBe("[Ĥóḿé ~~]")
    expect(i18n.t("footer.dataVersion", { version: "42" })).toContain("42")
    const plain = await createI18n("en")
    expect(plain.t("nav.home")).toBe("Home")
  })
})
