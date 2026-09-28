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

// Segmented pills never wrap, so every `options.*` label must stay within this width in every
// language; a CJK character is about 1 em wide, a Latin one about 0.55 em. Shorten the copy (or use
// the language's own name, as the language picker does) rather than widening the control.
const OPTION_BUDGET_EM = 6
// A whole segmented group must fit the 320px account menu (288px inside its padding): 13px text,
// 24px horizontal padding per pill, 4px of group padding and border.
const GROUP_BUDGET_PX = 288
const PILL_PADDING_PX = 24
const GROUP_CHROME_PX = 4
const FONT_PX = 13
const WIDE_CHAR =
  /[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Hangul}\u3000-\u303f\uff00-\uffef]/u

function estimateEm(text: string): number {
  let em = 0
  for (const char of text) em += WIDE_CHAR.test(char) ? 1 : char === " " ? 0.3 : 0.55
  return em
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

  it("keeps option labels short enough for segmented controls in every language", () => {
    for (const [lng, messages] of locales) {
      for (const [key, text] of messages) {
        if (!key.startsWith("options.")) continue
        expect(estimateEm(text), `${lng}:${key} ${JSON.stringify(text)}`).toBeLessThanOrEqual(
          OPTION_BUDGET_EM,
        )
      }
    }
  })

  it("keeps every segmented group narrow enough for the account menu in every language", () => {
    for (const [lng, messages] of locales) {
      const groups = new Map<string, number>()
      for (const [key, text] of messages) {
        const group = /^(options\.[^.]+)\./.exec(key)?.[1]
        if (!group) continue
        groups.set(group, (groups.get(group) ?? 0) + estimateEm(text) * FONT_PX + PILL_PADDING_PX)
      }
      for (const [group, width] of groups) {
        expect(width + GROUP_CHROME_PX, `${lng}:${group}`).toBeLessThanOrEqual(GROUP_BUDGET_PX)
      }
    }
  })

  it("estimates CJK text as wider than Latin text", () => {
    expect(estimateEm("繁體中文")).toBe(4)
    expect(estimateEm("English")).toBeCloseTo(3.85)
    expect(estimateEm("Traditional Chinese")).toBeGreaterThan(OPTION_BUDGET_EM)
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
