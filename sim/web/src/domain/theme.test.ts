import { describe, expect, it } from "vitest"

import { ACCENTS, defaultAccent, isAccent, isThemePref, resolveTheme, THEME_PREFS } from "./theme"

describe("resolveTheme", () => {
  it.each([
    // [theme pref, accent pref, system dark] -> [resolved theme, resolved accent]
    ["system", null, true, "dark", "amber"],
    ["system", null, false, "light", "teal"],
    ["light", null, true, "light", "teal"],
    ["dark", null, false, "dark", "amber"],
    ["system", "amber", false, "light", "amber"],
    ["system", "teal", true, "dark", "teal"],
    ["light", "red", true, "light", "red"],
    ["dark", "red", false, "dark", "red"],
    ["dark", "teal", true, "dark", "teal"],
    ["light", "amber", false, "light", "amber"],
  ] as const)("theme=%s accent=%s systemDark=%s -> %s/%s", (theme, accent, systemDark, t, a) => {
    expect(resolveTheme({ theme, accent }, systemDark)).toEqual({ theme: t, accent: a })
  })

  it("covers every theme × accent × system combination without throwing", () => {
    for (const theme of THEME_PREFS) {
      for (const accent of [...ACCENTS, null]) {
        for (const systemDark of [true, false]) {
          const resolved = resolveTheme({ theme, accent }, systemDark)
          expect(["light", "dark"]).toContain(resolved.theme)
          expect(ACCENTS).toContain(resolved.accent)
          if (accent) expect(resolved.accent).toBe(accent)
        }
      }
    }
  })
})

describe("defaults and guards", () => {
  it("defaults dark to amber and light to teal", () => {
    expect(defaultAccent("dark")).toBe("amber")
    expect(defaultAccent("light")).toBe("teal")
  })

  it("guards accept only the known values", () => {
    expect(isThemePref("system")).toBe(true)
    expect(isThemePref("auto")).toBe(false)
    expect(isThemePref(undefined)).toBe(false)
    expect(isAccent("red")).toBe(true)
    expect(isAccent("cyan")).toBe(false)
    expect(isAccent(null)).toBe(false)
  })
})
