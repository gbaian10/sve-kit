import { describe, expect, it } from "vitest"

import { resources } from "./resources"

// Characters that exist only in Simplified Chinese (not in Traditional, not in Japanese kanji we
// would ever want in the zh-TW file). A hit means a Simplified form slipped into the zh-TW copy.
// Keep this list to unambiguous characters; review catches the rest.
const SIMPLIFIED_ONLY =
  "们个为这么进还发电门问题时间关键东点无义体从产会将种资应说话语级则数据两术显图对开设习编辑际线条务备选错误议区处标签页块态状况网络与联删动载让单双变换记录转确认输项择类属经过复杂简卫获奖励费买卖币场环节击层战队伤现"

type Tree = { readonly [key: string]: string | Tree }

function flatten(tree: Tree, prefix = ""): Map<string, string> {
  const out = new Map<string, string>()
  for (const [key, value] of Object.entries(tree)) {
    const path = prefix ? `${prefix}.${key}` : key
    if (typeof value === "string") out.set(path, value)
    else for (const [k, v] of flatten(value, path)) out.set(k, v)
  }
  return out
}

function placeholders(text: string): string[] {
  return [...text.matchAll(/\{\{\s*([\w.-]+)\s*\}\}/g)].map((m) => m[1] ?? "").sort()
}

const locales = Object.entries(resources).map(
  ([lng, bundle]) => [lng, flatten(bundle.translation)] as const,
)
const reference = locales.find(([lng]) => lng === "zh-TW")
if (!reference) throw new Error("zh-TW locale missing")
const [, zhTW] = reference

describe("i18n static checks", () => {
  it("has the same keys in every locale", () => {
    for (const [lng, messages] of locales) {
      expect([...messages.keys()].sort(), lng).toEqual([...zhTW.keys()].sort())
    }
  })

  it("uses the same interpolation placeholders for a key in every locale", () => {
    for (const [key, text] of zhTW) {
      const expected = placeholders(text)
      for (const [lng, messages] of locales) {
        expect(placeholders(messages.get(key) ?? ""), `${lng}:${key}`).toEqual(expected)
      }
    }
  })

  it("has no unclosed placeholder braces", () => {
    for (const [lng, messages] of locales) {
      for (const [key, text] of messages) {
        const opens = text.split("{{").length - 1
        const closes = text.split("}}").length - 1
        expect(opens, `${lng}:${key}`).toBe(closes)
      }
    }
  })

  it("keeps Simplified-only characters out of zh-TW", () => {
    const banned = new Set(SIMPLIFIED_ONLY)
    for (const [key, text] of zhTW) {
      const hits = Array.from(text).filter((ch) => banned.has(ch))
      expect(hits, key).toEqual([])
    }
  })

  it("the ban list itself is de-duplicated", () => {
    expect(new Set(SIMPLIFIED_ONLY).size).toBe(SIMPLIFIED_ONLY.length)
  })
})
