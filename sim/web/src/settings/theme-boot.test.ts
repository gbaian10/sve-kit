import { afterEach, describe, expect, it, vi } from "vitest"

import bootSource from "../../public/theme-boot.js?raw"
import { applyThemeAttributes } from "../app/theme-attributes"
import { ACCENTS, THEME_PREFS } from "../domain/theme"
import { PREFS_KEY } from "./prefs"

const html = document.documentElement

function runBoot(): void {
  // eslint-disable-next-line @typescript-eslint/no-implied-eval -- the boot file is plain JS served outside the bundle; this runs it as the browser would
  const run = new Function(bootSource) as () => void
  run()
}

function attrs(): { theme: string | null; accent: string | null } {
  return { theme: html.getAttribute("data-theme"), accent: html.getAttribute("data-accent") }
}

afterEach(() => {
  vi.restoreAllMocks()
  localStorage.clear()
  html.removeAttribute("data-theme")
  html.removeAttribute("data-accent")
})

describe("public/theme-boot.js", () => {
  it("applies a stored explicit theme and accent", () => {
    localStorage.setItem(PREFS_KEY, JSON.stringify({ v: 1, theme: "dark", accent: "red" }))
    runBoot()
    expect(attrs()).toEqual({ theme: "dark", accent: "red" })
  })

  it("leaves the system theme and default accent as attribute-less", () => {
    html.setAttribute("data-theme", "dark")
    html.setAttribute("data-accent", "teal")
    localStorage.setItem(PREFS_KEY, JSON.stringify({ v: 1, theme: "system", accent: null }))
    runBoot()
    expect(attrs()).toEqual({ theme: null, accent: null })
  })

  it.each([
    "{not json",
    "[]",
    "null",
    JSON.stringify({ theme: "sepia", accent: "cyan" }),
    JSON.stringify({ theme: "dark", accent: "red" }),
    JSON.stringify({ v: 2, theme: "dark", accent: "red" }),
  ])("falls back to the system theme for unusable content %s", (raw) => {
    localStorage.setItem(PREFS_KEY, raw)
    runBoot()
    expect(attrs()).toEqual({ theme: null, accent: null })
  })

  it("survives a localStorage getter that throws", () => {
    const descriptor = Object.getOwnPropertyDescriptor(window, "localStorage")
    Object.defineProperty(window, "localStorage", {
      configurable: true,
      get: () => {
        throw new DOMException("blocked", "SecurityError")
      },
    })
    try {
      expect(() => {
        runBoot()
      }).not.toThrow()
      expect(attrs()).toEqual({ theme: null, accent: null })
    } finally {
      if (descriptor) Object.defineProperty(window, "localStorage", descriptor)
    }
  })

  it("agrees with applyThemeAttributes for every theme × accent", () => {
    for (const theme of THEME_PREFS) {
      for (const accent of [...ACCENTS, null]) {
        localStorage.setItem(PREFS_KEY, JSON.stringify({ v: 1, theme, accent }))
        runBoot()
        const fromBoot = attrs()
        html.removeAttribute("data-theme")
        html.removeAttribute("data-accent")
        applyThemeAttributes({ theme, accent })
        expect(attrs(), `${theme}/${String(accent)}`).toEqual(fromBoot)
      }
    }
  })
})
