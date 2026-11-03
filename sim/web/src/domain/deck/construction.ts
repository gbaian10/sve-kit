/**
 * Deck construction checks over a construction index (docs/sim/deck-construction.md). Pure: the
 * deck builder, the lobby and the match share these rules and the same JSON cases.
 */

export type Zone = "none" | "leader" | "main" | "evolve"
type Section = "main" | "evolve"
type Scope = Section | "all"

export interface IndexCard {
  /** Rules name code; cards with one code share the same-name limit (CR 6.1.1.4). */
  readonly name: number
  /** Class code, `neutral` included; null only for cards that never enter a deck. */
  readonly classCode: string | null
  readonly titles: readonly string[]
  readonly zone: Zone
  /** Limits the card text sets per deck (CR 6.1.2). */
  readonly copies?: Readonly<Partial<Record<Section, number>>>
  /** In the main deck, every other name is limited to one copy except these (CR 6.1.2). */
  readonly singletonExcept?: ReadonlySet<number>
  readonly startAmulet: boolean
}

interface RestrictionMember {
  readonly name: number
  readonly scope: Scope
  readonly option: number
}

export interface Restriction {
  readonly id: string
  readonly kind: "copy_limit" | "choice_group"
  readonly from: string
  readonly until: string | null
  readonly max: number | null
  readonly maxGroups: number | null
  readonly members: readonly RestrictionMember[]
}

export interface FormatRules {
  /** Date ranges whose restriction lists are known complete; `until` is exclusive. */
  readonly coverage: readonly (readonly [string, string | null])[]
  readonly restrictions: readonly Restriction[]
}

export interface ConstructionIndex {
  readonly card: (cardId: string) => IndexCard | undefined
  /** Title code → the rule set of CR 14 it brings (only `vanguard` today). */
  readonly titleRules: ReadonlyMap<string, string>
  readonly formats: ReadonlyMap<string, FormatRules>
}

type DeckBasis =
  | { readonly kind: "class"; readonly classCode?: string }
  | { readonly kind: "title"; readonly titleCode: string }

export interface DeckInput {
  readonly basis: DeckBasis
  readonly leader: string | null
  readonly main: readonly (readonly [string, number])[]
  readonly evolve: readonly (readonly [string, number])[]
}

export interface CheckContext {
  readonly format: string
  /** `YYYY-MM-DD`; picks the restrictions in force. */
  readonly date: string
}

export type ViolationCode =
  | "leaderMissing"
  | "leaderNotLeader"
  | "mainTooFew"
  | "mainTooMany"
  | "mainWrongCard"
  | "evolveTooMany"
  | "evolveWrongCard"
  | "sameNameOver"
  | "leaderClassMismatch"
  | "classMismatch"
  | "leaderTitleMismatch"
  | "titleMismatch"
  | "startAmuletMissing"
  | "startAmuletNames"
  | "unknownCard"
  | "banned"
  | "restricted"
  | "choiceRestricted"
  | "formatUnsupported"

export type NoteCode = "triggerClassUnchecked" | "restrictionsUnknown"

interface Violation {
  readonly code: ViolationCode
  /** Comprehensive Rules clause, `restriction` for the official lists, `format` for formats. */
  readonly clause: string
  readonly cards: readonly string[]
  /** `name` values are rules name codes. */
  readonly values: Readonly<Record<string, string | number>>
}

interface Note {
  readonly code: NoteCode
  readonly clause: string
}

export interface CheckResult {
  readonly legal: boolean
  readonly violations: readonly Violation[]
  readonly notes: readonly Note[]
}

export const MAIN_MIN = 40
export const MAIN_MAX = 50
export const EVOLVE_MAX = 10
export const SAME_NAME_LIMIT = 3
const NEUTRAL = "neutral"

const inForce = (day: string, from: string, until: string | null) =>
  from <= day && (until === null || day < until)

/** Every construction violation of a deck; legal means none were found (notes never count). */
export function checkConstruction(
  deck: DeckInput,
  index: ConstructionIndex,
  context: CheckContext,
): CheckResult {
  const violations: Violation[] = []
  const notes: Note[] = []
  const add = (
    code: ViolationCode,
    clause: string,
    cards: Iterable<string> = [],
    values: Record<string, string | number> = {},
  ) => {
    violations.push({ code, clause, cards: [...new Set(cards)], values })
  }
  const rules = index.formats.get(context.format)
  if (!rules) {
    add("formatUnsupported", "format", [], { format: context.format })
    return { legal: false, violations, notes }
  }

  const unknown = new Set<string>()
  let leader: IndexCard | undefined
  if (deck.leader === null) add("leaderMissing", "6.1.1.1")
  else {
    const card = index.card(deck.leader)
    if (!card) unknown.add(deck.leader)
    else if (card.zone !== "leader") add("leaderNotLeader", "6.1.1.1", [deck.leader])
    else leader = card
  }

  const sections = { main: known(deck.main), evolve: known(deck.evolve) }
  function known(entries: DeckInput["main"]) {
    const out: { readonly id: string; readonly count: number; readonly card: IndexCard }[] = []
    for (const [id, count] of entries) {
      const card = index.card(id)
      if (card) out.push({ id, count, card })
      else unknown.add(id)
    }
    return out
  }
  for (const id of unknown) add("unknownCard", "6.1.1", [id])

  const total = (entries: DeckInput["main"]) => entries.reduce((sum, [, count]) => sum + count, 0)
  const mainCount = total(deck.main)
  const evolveCount = total(deck.evolve)
  if (mainCount < MAIN_MIN) add("mainTooFew", "6.1.1.2", [], { count: mainCount, min: MAIN_MIN })
  if (mainCount > MAIN_MAX) add("mainTooMany", "6.1.1.2", [], { count: mainCount, max: MAIN_MAX })
  if (evolveCount > EVOLVE_MAX)
    add("evolveTooMany", "6.1.1.3", [], { count: evolveCount, max: EVOLVE_MAX })
  for (const entry of sections.main)
    if (entry.card.zone !== "main") add("mainWrongCard", "6.1.1.2", [entry.id])
  for (const entry of sections.evolve)
    if (entry.card.zone !== "evolve") add("evolveWrongCard", "6.1.1.3", [entry.id])

  checkSameName(sections, add)

  const every = [...sections.main, ...sections.evolve]
  const firstById = new Map(every.map((entry) => [entry.id, entry.card] as const))
  if (deck.basis.kind === "class") {
    const declared = deck.basis.classCode
    if (
      leader !== undefined &&
      deck.leader !== null &&
      declared !== undefined &&
      declared !== NEUTRAL &&
      declared !== leader.classCode
    )
      add("leaderClassMismatch", "6.1.1.5.1", [deck.leader], {
        deckClass: declared,
        leaderClass: leader.classCode ?? NEUTRAL,
      })
    const deckClass = leader ? leader.classCode : declared
    if (deckClass !== undefined && deckClass !== null)
      for (const [id, card] of firstById)
        if (card.classCode !== NEUTRAL && card.classCode !== deckClass)
          add("classMismatch", "6.1.1.5.1", [id], {
            cardClass: card.classCode ?? NEUTRAL,
            deckClass,
          })
  } else {
    const title = deck.basis.titleCode
    if (leader && deck.leader !== null && !leader.titles.includes(title))
      add("leaderTitleMismatch", "6.1.1.5.2", [deck.leader], { title })
    for (const [id, card] of firstById)
      if (!card.titles.includes(title)) add("titleMismatch", "6.1.1.5.2", [id], { title })
    if (index.titleRules.get(title) === "vanguard") {
      if (!sections.main.some((entry) => entry.card.startAmulet))
        add("startAmuletMissing", "14.4.2.1.1")
      notes.push({ code: "triggerClassUnchecked", clause: "14.4.2.1.2" })
    }
  }
  const amulets = every.filter((entry) => entry.card.startAmulet)
  if (new Set(amulets.map((entry) => entry.card.name)).size > 1)
    add(
      "startAmuletNames",
      "14.4.4.1.1",
      amulets.map((entry) => entry.id),
    )

  if (!rules.coverage.some(([from, until]) => inForce(context.date, from, until)))
    notes.push({ code: "restrictionsUnknown", clause: "restriction" })
  for (const restriction of rules.restrictions) {
    if (!inForce(context.date, restriction.from, restriction.until)) continue
    const counted: { readonly id: string; readonly count: number; readonly option: number }[] = []
    for (const member of restriction.members)
      for (const section of member.scope === "all" ? (["main", "evolve"] as const) : [member.scope])
        for (const entry of sections[section])
          if (entry.card.name === member.name)
            counted.push({ id: entry.id, count: entry.count, option: member.option })
    if (counted.length === 0) continue
    const ids = counted.map((entry) => entry.id)
    if (restriction.kind === "copy_limit") {
      const count = counted.reduce((sum, entry) => sum + entry.count, 0)
      const max = restriction.max ?? 0
      if (count > max)
        add(max === 0 ? "banned" : "restricted", "restriction", ids, {
          name: restriction.members[0]?.name ?? -1,
          count,
          max,
          from: restriction.from,
        })
    } else {
      const options = new Set(counted.map((entry) => entry.option)).size
      const max = restriction.maxGroups ?? 0
      if (options > max)
        add("choiceRestricted", "restriction", ids, { count: options, max, from: restriction.from })
    }
  }
  return { legal: violations.length === 0, violations, notes }
}

type Entry = { readonly id: string; readonly count: number; readonly card: IndexCard }

/** CR 6.1.1.4 per deck, with the text overrides of CR 6.1.2 (spec §3.1). */
function checkSameName(
  sections: Readonly<Record<Section, readonly Entry[]>>,
  add: (
    code: ViolationCode,
    clause: string,
    cards: Iterable<string>,
    values: Record<string, string | number>,
  ) => void,
) {
  const singletons = sections.main
    .map((entry) => entry.card.singletonExcept)
    .filter((exempt) => exempt !== undefined)
  for (const section of ["main", "evolve"] as const) {
    const groups = new Map<number, { count: number; ids: string[]; own: number | undefined }>()
    for (const entry of sections[section]) {
      const group = groups.get(entry.card.name) ?? { count: 0, ids: [], own: undefined }
      group.count += entry.count
      group.ids.push(entry.id)
      group.own ??= entry.card.copies?.[section]
      groups.set(entry.card.name, group)
    }
    for (const [name, group] of groups) {
      const singleton =
        singletons.length > 0 && !singletons.every((exempt) => exempt.has(name)) ? 1 : undefined
      const limit = singleton ?? group.own ?? SAME_NAME_LIMIT
      if (group.count > limit)
        add(
          "sameNameOver",
          singleton !== undefined || group.own !== undefined ? "6.1.2" : "6.1.1.4",
          group.ids,
          { name, section, count: group.count, limit },
        )
    }
  }
}
