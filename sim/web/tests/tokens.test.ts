// @vitest-environment node
import { readFileSync } from "node:fs"
import path from "node:path"

import postcss, { type AtRule, type Root, type Rule } from "postcss"
import { describe, expect, it } from "vitest"

// Every base colour token must be defined in the three theme contexts, and every accent must be
// defined in each of them too; see README "Theme tokens".
const tokensFile = path.resolve(import.meta.dirname, "../src/styles/tokens.css")
const root: Root = postcss.parse(readFileSync(tokensFile, "utf8"))

const ACCENTS = ["amber", "teal", "red"]
const ACCENT_TOKENS = ["--accent", "--accent-ink", "--accent-text", "--accent-soft"].sort()

const LIGHT = ":root"
const DARK_ATTR = ':root[data-theme="dark"]'
const DARK_MEDIA = ':root:not([data-theme="light"])'
const DARK_MEDIA_PARAMS = "(prefers-color-scheme: dark)"

function isDarkMedia(node: Rule): boolean {
  const parent = node.parent
  return parent?.type === "atrule" && (parent as AtRule).params === DARK_MEDIA_PARAMS
}

/** Custom property names declared directly inside the rule with this exact selector and context. */
function declared(selector: string, inDarkMedia: boolean): string[] {
  const names: string[] = []
  root.walkRules((rule) => {
    if (rule.selector !== selector || isDarkMedia(rule) !== inDarkMedia) return
    rule.each((child) => {
      if (child.type === "decl" && child.prop.startsWith("--")) {
        names.push(child.prop)
      }
    })
  })
  return names
}

const light = declared(LIGHT, false)
const darkAttr = declared(DARK_ATTR, false)
const darkMedia = declared(DARK_MEDIA, true)
const base = light.filter((name) => !name.startsWith("--accent")).sort()

describe("tokens.css theme contexts", () => {
  it("defines the same base tokens in all three theme selectors", () => {
    expect(base.length).toBeGreaterThan(20)
    expect(darkAttr.filter((n) => !n.startsWith("--accent")).sort()).toEqual(base)
    expect(darkMedia.filter((n) => !n.startsWith("--accent")).sort()).toEqual(base)
  })

  it("declares each token once per context", () => {
    for (const list of [light, darkAttr, darkMedia]) {
      expect(new Set(list).size).toBe(list.length)
    }
  })

  it("sets color-scheme in every theme context", () => {
    for (const [selector, inMedia] of [
      [LIGHT, false],
      [DARK_ATTR, false],
      [DARK_MEDIA, true],
    ] as const) {
      const values: string[] = []
      root.walkRules((rule) => {
        if (rule.selector !== selector || isDarkMedia(rule) !== inMedia) return
        rule.walkDecls("color-scheme", (decl) => {
          values.push(decl.value)
        })
      })
      expect(values, selector).toHaveLength(1)
    }
  })

  it("gives every theme context a default accent", () => {
    for (const list of [light, darkAttr, darkMedia]) {
      expect(list.filter((n) => n.startsWith("--accent")).sort()).toEqual(ACCENT_TOKENS)
    }
  })

  it("defines every accent in every theme context", () => {
    for (const accent of ACCENTS) {
      const attr = `[data-accent="${accent}"]`
      expect(declared(`${LIGHT}${attr}`, false).sort(), `${accent} light`).toEqual(ACCENT_TOKENS)
      expect(declared(`${DARK_ATTR}${attr}`, false).sort(), `${accent} dark attr`).toEqual(
        ACCENT_TOKENS,
      )
      expect(declared(`${DARK_MEDIA}${attr}`, true).sort(), `${accent} dark media`).toEqual(
        ACCENT_TOKENS,
      )
    }
  })

  it("keeps every colour literal inside tokens.css (no var() indirection needed there)", () => {
    root.walkDecls((decl) => {
      if (!decl.prop.startsWith("--")) return
      if (decl.prop.startsWith("--font")) return
      expect(decl.value, decl.prop).not.toMatch(/^var\(/)
    })
  })
})
