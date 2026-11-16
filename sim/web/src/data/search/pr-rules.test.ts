// @vitest-environment node
import { expect, it } from "vitest"

import type { JsonObject } from "../format-v3/json"
import type { Fragment } from "../format-v3/reader"
import { PrOnlyRules } from "./pr-rules"

interface Card {
  readonly id: string
  readonly pr: boolean
  readonly name: string
  readonly body?: string
  readonly extra?: JsonObject
  readonly sections?: readonly string[]
  readonly back?: string
  readonly en?: string
}
function fragment(table: string, rows: JsonObject[]): Fragment {
  return {
    table,
    rows,
    file: table,
    identity: table,
    value: { owner: { kind: "home_set", id: "PR" }, base: null },
  }
}
function catalog(cards: readonly Card[]): Fragment[] {
  const texts: JsonObject[] = []
  const faces: JsonObject[] = []
  const revisions: JsonObject[] = []
  const printings: Fragment[] = []
  for (const card of cards) {
    for (const region of ["jp", "en"] as const) {
      printings.push({
        ...fragment("printing", [
          {
            id: `${card.id}:${region}`,
            card_id: card.id,
            card_no: `${card.pr ? "PR" : "BP01"}-${card.id}`,
            region,
          },
        ]),
        value: { owner: { kind: "home_set", id: card.pr ? "PR" : "BP01" }, base: null },
      })
    }
    const sides = card.back === undefined ? [card.body] : [card.body, card.back]
    for (const [ordinal, body] of sides.entries()) {
      const face = `${card.id}:f:${String(ordinal)}`
      faces.push({
        id: face,
        card_id: card.id,
        ordinal,
        side: ordinal ? "back" : "front",
        current: ["jp", "en"].map((region) => ({ region, revision_id: `${face}:${region}` })),
        wording: [],
      })
      for (const region of ["jp", "en"] as const) {
        const id = `${face}:${region}`,
          effect = region === "en" && ordinal === 0 ? (card.en ?? body) : body
        texts.push({ id: `${id}:name`, lang: region === "jp" ? "ja" : "en", text: card.name })
        if (effect !== undefined)
          texts.push({ id: `${id}:body`, lang: region === "jp" ? "ja" : "en", text: effect })
        const sections = (card.sections ?? []).map((text, i) => {
          const textId = `${id}:section:${String(i)}`
          texts.push({ id: textId, lang: region === "jp" ? "ja" : "en", text })
          return { ordinal: i, kind: "unknown", text_unit_id: textId }
        })
        revisions.push({
          id,
          face_id: face,
          region,
          name_unit_id: `${id}:name`,
          effect_unit_id: `${id}:body`,
          cost: 1,
          attack: 1,
          defense: 1,
          type_code: "follower",
          traits: [],
          special_kinds: [],
          sections,
          ...card.extra,
        })
      }
    }
  }
  return [
    fragment("product_family", [
      { id: "PR", public_code: "PR" },
      { id: "BP01", public_code: "BP01" },
    ]),
    fragment("face", faces),
    fragment("face_revision", revisions),
    fragment("text_unit", texts),
    ...printings,
  ]
}
const excluded = (cards: readonly Card[]) => new PrOnlyRules(catalog(cards)).excluded([])

it("excludes differently named PR rules without changing their permanent identities", () => {
  const cards = [
    { id: "regular", pr: false, name: "Sun", body: "Sun gains +1/+1. Draw 1 card." },
    { id: "same-rules", pr: true, name: "Moon", body: "Moon　gains ＋１/＋１！\nDraw 1 card。" },
    { id: "unique", pr: true, name: "Star", body: "Star gains +1/+1. Draw 2 cards." },
  ]
  const input = catalog(cards),
    before = structuredClone(input)
  expect([...new PrOnlyRules(input).excluded([])]).toEqual(["same-rules"])
  expect(input).toEqual(before)
})

it.each([
  ["type", { type_code: "spell" }],
  ["cost", { cost: 2 }],
  ["attack", { attack: 2 }],
  ["defense", { defense: 2 }],
  ["traits", { traits: ["other"] }],
  ["evolve", { special_kinds: ["evolve"] }],
] satisfies [string, JsonObject][])("keeps a PR card when its %s differs", (_field, extra) => {
  expect(
    excluded([
      { id: "base", pr: false, name: "Base", body: "Draw 1 card." },
      { id: "pr", pr: true, name: "PR", body: "Draw 1 card.", extra },
    ]).has("pr"),
  ).toBe(false)
})

it.each(["ep", "sep", "token"])(
  "excludes %s even when no equivalent non-PR printing exists",
  (kind) => {
    const extra = kind === "token" ? { special_kinds: ["token"] } : { type_code: kind }
    expect(
      excluded([{ id: "pr", pr: true, name: "Special", body: "Unique.", extra }]).has("pr"),
    ).toBe(true)
  },
)

it("keeps signs, decimal quantities, and references to other cards significant", () => {
  const regular = { id: "base", pr: false, name: "Base", body: "Gain 1.5. Lose -1. Find Other." }
  for (const body of [
    "Gain 15. Lose -1. Find Other.",
    "Gain 1.5. Lose 1. Find Other.",
    "Gain 1.5. Lose +1. Find Other.",
    "Gain 1.5. Lose -1. Find Someone.",
  ])
    expect(excluded([regular, { id: "pr", pr: true, name: "Promo", body }]).has("pr")).toBe(false)
})

it("compares auxiliary rules and every face instead of only the front body", () => {
  const base = {
    id: "base",
    pr: false,
    name: "Base",
    body: "Front.",
    back: "Back.",
    sections: ["Extra rule."],
  }
  expect(
    excluded([
      base,
      { ...base, id: "same", pr: true, name: "Same" },
      { ...base, id: "back", pr: true, name: "Back differs", back: "Different." },
      { ...base, id: "section", pr: true, name: "Section differs", sections: ["Different rule."] },
    ]),
  ).toEqual(new Set(["same"]))
})

it("uses a known shared card identity across regions without matching different-language text", () => {
  expect(
    excluded([
      {
        id: "regular",
        pr: false,
        name: "Base",
        body: "Draw 1 card.",
        en: "A different official formulation.",
      },
      {
        id: "promo",
        pr: true,
        name: "Promo",
        body: "Draw 1 card.",
        en: "Another regional formulation.",
      },
    ]),
  ).toEqual(new Set(["promo"]))
})

it("does not claim exclusivity when a structural peer's rules are unavailable", () => {
  expect(
    excluded([
      { id: "regular", pr: false, name: "Base" },
      { id: "promo", pr: true, name: "Promo", body: "Known rule." },
    ]),
  ).toEqual(new Set(["promo"]))
})

it("matches known empty leader rules and ignores class when the stated rule fields agree", () => {
  expect(
    excluded([
      {
        id: "regular",
        pr: false,
        name: "Base",
        body: "",
        extra: { type_code: "leader", class_code: "elf" },
      },
      {
        id: "promo",
        pr: true,
        name: "Promo",
        body: "",
        extra: { type_code: "leader", class_code: "royal" },
      },
    ]),
  ).toEqual(new Set(["promo"]))
})

it("does not replace a short own name inside a different card name", () => {
  expect(
    excluded([
      { id: "regular", pr: false, name: "Fire", body: "Find Fireball." },
      { id: "promo", pr: true, name: "Ice", body: "Find Iceball." },
    ]).has("promo"),
  ).toBe(false)
})

it("normalizes apostrophe typography without conflating singular and plural possession", () => {
  expect(
    excluded([
      { id: "regular", pr: false, name: "Base", body: "Increase your card's attack." },
      { id: "typography", pr: true, name: "Promo", body: "Increase your card’s attack." },
      { id: "plural", pr: true, name: "Plural", body: "Increase your cards' attack." },
    ]),
  ).toEqual(new Set(["typography"]))
})
