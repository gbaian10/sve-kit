// @vitest-environment node
import { describe, expect, it } from "vitest"

import { resources } from "./resources"

// Every source file except tests, fixtures and the locale files themselves.
const sources = import.meta.glob<string>("../**/*.{ts,tsx}", {
  query: "?raw",
  import: "default",
  eager: true,
})

type Tree = { readonly [key: string]: string | Tree }

function keys(tree: Tree, prefix = ""): string[] {
  return Object.entries(tree).flatMap(([key, value]) => {
    const path = prefix ? `${prefix}.${key}` : key
    return typeof value === "string" ? [path] : keys(value, path)
  })
}

const SKIP = /(?:\.test\.tsx?|test-setup\.ts|test-utils\.tsx|\/i18n\/locales\/[^/]+)$/
const LITERAL = /(["'])((?:(?!\1)[^\\\n]|\\.)*)\1/g
// Only a translation call counts as a reference: t("nav.home") or t(`nav.${key}`). A key that
// merely equals some other string (a route name, a setting value) is still dead.
const CALL_KEY = /\bt\(\s*(["'])((?:(?!\1)[^\\\n]|\\.)*)\1/g
const CALL_HEAD = /\bt\(\s*`([\w.-]+\.)\$\{/g

const literals = new Set<string>()
const called = new Set<string>()
const prefixes = new Set<string>()
for (const [file, text] of Object.entries(sources)) {
  if (SKIP.test(file)) continue
  for (const match of text.matchAll(LITERAL)) literals.add(match[2] ?? "")
  for (const match of text.matchAll(CALL_KEY)) called.add(match[2] ?? "")
  for (const match of text.matchAll(CALL_HEAD)) prefixes.add(match[1] ?? "")
}

const localeKeys = keys(resources["zh-TW"].translation)
const used = (key: string) =>
  called.has(key) || [...prefixes].some((prefix) => key.startsWith(prefix))

describe("i18n key usage", () => {
  it("scans the application sources", () => {
    expect(Object.keys(sources).some((file) => file.endsWith("/app/AppShell.tsx"))).toBe(true)
    expect(literals.size).toBeGreaterThan(50)
  })

  // A key no t() call references is dead copy in three languages; delete it with the UI it served.
  it("has no locale key that no translation call references", () => {
    expect(localeKeys.filter((key) => !used(key))).toEqual([])
  })

  // Typos in t("...") compile fine but render the raw key; this is the check the types cannot do.
  it("has no dotted string literal that looks like a locale key but is missing", () => {
    const roots = new Set(localeKeys.map((key) => key.split(".")[0] ?? ""))
    const suspicious = [...literals].filter((literal) => {
      const root = literal.split(".")[0] ?? ""
      return (
        /^[a-z][\w-]*(?:\.[\w-]+)+$/.test(literal) &&
        roots.has(root) &&
        !localeKeys.includes(literal)
      )
    })
    expect(suspicious).toEqual([])
  })
})
