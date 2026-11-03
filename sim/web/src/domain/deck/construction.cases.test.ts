import { describe, expect, it } from "vitest"

import cases from "../../../../../docs/sim/deck-construction-cases.json"
import {
  checkConstruction,
  type ConstructionIndex,
  type FormatRules,
  type IndexCard,
  type Zone,
} from "./construction"

// The shared cases describe cards in the logical form of the spec (docs/sim/deck-construction.md
// §1.2, §9); this harness gives names integer codes the way the exported index does.
interface LogicalCard {
  readonly name: string
  readonly class: string
  readonly titles: readonly string[]
  readonly zone: string
  readonly copies?: Readonly<Record<string, number>>
  readonly singleton_except?: readonly string[]
  readonly start_amulet?: boolean
}
interface LogicalRestriction {
  readonly id: string
  readonly kind: string
  readonly from: string
  readonly until?: string | null
  readonly max?: number
  readonly max_groups?: number
  readonly members: readonly {
    readonly name: string
    readonly scope: string
    readonly option?: number
  }[]
}
interface Expected {
  readonly code: string
  readonly clause: string
  readonly cards?: readonly string[]
  readonly values?: Readonly<Record<string, string | number | undefined>> | undefined
}

const codes = new Map<string, number>()
const code = (name: string) => {
  const known = codes.get(name)
  if (known !== undefined) return known
  codes.set(name, codes.size)
  return codes.size - 1
}

const logical = new Map<string, LogicalCard>(
  Object.entries(cases.index.cards as Readonly<Record<string, LogicalCard>>),
)
const fillers = new Map<string, string[]>()
for (const [set, spec] of Object.entries(cases.fillers)) {
  const ids: string[] = []
  for (let i = 1; i <= spec.count; i++) {
    const number = String(i).padStart(2, "0")
    const id = `FILL-${set.toUpperCase()}-${number}`
    logical.set(id, {
      name: `合成：${set} 填充 ${number}`,
      class: spec.class,
      titles: spec.titles,
      zone: "main",
    })
    ids.push(id)
  }
  fillers.set(set, ids)
}

const indexCards = new Map<string, IndexCard>(
  [...logical].map(([id, card]) => [
    id,
    {
      name: code(card.name),
      classCode: card.class,
      titles: card.titles,
      zone: card.zone as Zone,
      ...(card.copies ? { copies: card.copies } : {}),
      ...(card.singleton_except
        ? { singletonExcept: new Set(card.singleton_except.map(code)) }
        : {}),
      startAmulet: card.start_amulet ?? false,
    },
  ]),
)
const formats = new Map<string, FormatRules>(
  Object.entries(cases.index.formats).map(([format, rules]) => [
    format,
    {
      coverage: (rules.coverage as readonly { from: string; until?: string | null }[]).map(
        (range) => [range.from, range.until ?? null] as const,
      ),
      restrictions: (rules.restrictions as readonly LogicalRestriction[]).map((restriction) => ({
        id: restriction.id,
        kind: restriction.kind as "copy_limit" | "choice_group",
        from: restriction.from,
        until: restriction.until ?? null,
        max: restriction.max ?? null,
        maxGroups: restriction.max_groups ?? null,
        members: restriction.members.map((member) => ({
          name: code(member.name),
          scope: member.scope as "main" | "evolve" | "all",
          option: member.option ?? 0,
        })),
      })),
    },
  ]),
)
const index: ConstructionIndex = {
  card: (id) => indexCards.get(id),
  titleRules: new Map(Object.entries(cases.index.title_rules)),
  formats,
}
const nameOf = new Map([...codes].map(([name, value]) => [value, name]))

type Entries = readonly (readonly [string, number])[]
const entries = (value: unknown): Entries => value as Entries

function deckOf(deck: (typeof cases.cases)[number]["deck"]) {
  const main = [...entries(deck.main)]
  const fill = "fill" in deck ? deck.fill : undefined
  if (fill) {
    let left = fill.count
    for (const id of fillers.get(fill.set) ?? []) {
      if (left <= 0) break
      main.push([id, Math.min(3, left)])
      left -= 3
    }
  }
  const basis =
    deck.basis.kind === "class"
      ? {
          kind: "class" as const,
          ...("class" in deck.basis ? { classCode: deck.basis.class } : {}),
        }
      : { kind: "title" as const, titleCode: "title" in deck.basis ? deck.basis.title : "" }
  return { basis, leader: deck.leader, main, evolve: entries(deck.evolve) }
}

/** The result in the cases' terms: rules names back to their strings, card lists as sets. */
function comparable(items: readonly Expected[], actual: boolean) {
  return items
    .map((item) => ({
      code: item.code,
      clause: item.clause,
      cards: [...(item.cards ?? [])].sort(),
      ...(item.values
        ? {
            values: Object.fromEntries(
              Object.entries(item.values).map(([key, value]) => [
                key,
                actual && key === "name" && typeof value === "number" ? nameOf.get(value) : value,
              ]),
            ),
          }
        : {}),
    }))
    .sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b)))
}

describe("shared deck construction cases", () => {
  it.each(cases.cases.map((item) => [item.id, item] as const))("%s", (_id, item) => {
    const result = checkConstruction(deckOf(item.deck), index, item.context)
    const expectedViolations = item.expect.violations as readonly Expected[]
    const expected = comparable(expectedViolations, false)
    // Only the values a case names are compared.
    const actual = comparable(
      result.violations.map((violation) => {
        const wanted = expectedViolations.find(
          (candidate) =>
            candidate.code === violation.code &&
            [...(candidate.cards ?? [])].sort().join() === [...violation.cards].sort().join(),
        )?.values
        return {
          ...violation,
          values: wanted
            ? Object.fromEntries(Object.keys(wanted).map((key) => [key, violation.values[key]]))
            : undefined,
        }
      }),
      true,
    )
    expect(actual).toEqual(expected)
    expect(comparable(result.notes, true)).toEqual(comparable(item.expect.notes, false))
    expect(result.legal).toBe(item.expect.legal)
  })
})
