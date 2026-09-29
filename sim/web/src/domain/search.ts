import { type CardNoKey, cardNoKey, cardNoLookupKey } from "./cardNo"
import { compareCodePoints, normalizeText } from "./normalize"
import { NEUTRAL_CLASS, type QuerySort, type QueryState } from "./query/model"

// Search over the bootstrap only (architecture §4.7): the data layer flattens each card into one
// entry, and everything here is a pure function of those entries, so ranking is testable without a
// snapshot and identical between the suggest list and the results page.
export type Region = "jp" | "en"
export type TextLang = "ja" | "en" | "zh-Hant"

export interface SearchName {
  readonly lang: TextLang
  readonly text: string
  readonly normalized: string
}

export interface FaceStats {
  readonly typeCode: string
  readonly cost: number | null
  readonly attack: number | null
  readonly defense: number | null
  /** The original name of this region's front face, for name sorting. */
  readonly name: string
}

export interface SearchPrinting {
  readonly id: string
  readonly region: Region
  readonly cardNo: string
  readonly rarity: string | null
  /** `standard`, `alt`, `signed`… (printing.variant_key). */
  readonly variant: string
  /** The front face's art id; printings of one art group together in the art unit. */
  readonly artId: string | null
  /** Earliest product release date (YYYY-MM-DD) of this printing, when known. */
  readonly releasedOn: string | null
  /** `cardNoLookupKey` of the printed number, or null when it does not parse. */
  readonly lookupKey: string | null
  /** `cardNoKey` of the printed number, for prefix matching by set and leading digits. */
  readonly key: CardNoKey | null
  /** The printed number normalized, for prefix matching on the set code alone. */
  readonly flat: string
}

export interface SearchEntry {
  readonly cardId: string
  /** Vocabulary class code; null for neutral cards. */
  readonly classCode: string | null
  readonly setId: string
  /** Position in the snapshot's canonical order (set, then number). */
  readonly order: number
  readonly names: readonly SearchName[]
  readonly aliases: readonly SearchName[]
  readonly printings: readonly SearchPrinting[]
  /** The region view's default printing per region, when the card is released there. */
  readonly defaultPrinting: Partial<Record<Region, string>>
  /** Set code (product_family.code) for the `set` facet. */
  readonly setCode: string
  /** Front-face stats per region the card is released in. */
  readonly faces: Partial<Record<Region, FaceStats>>
  /** Mechanic tri-state for a keyword as seen from a region (architecture §4.6). */
  readonly mechanic: (keywordId: string, region: Region) => "present" | "absent" | "unknown"
}

export type MatchedField = "cardNo" | "name" | "alias"

export interface Match {
  readonly field: MatchedField
  /** Lower is better: 0 exact/loose card number, 1 number prefix, 2 name prefix, 3 name contains. */
  readonly rank: number
  readonly printingId: string
}

export interface SearchOptions {
  readonly edition: Region
  /** Set codes known to the snapshot, so a number typed without separators splits correctly. */
  readonly sets: ReadonlySet<string>
}

export interface Suggestion extends Match {
  readonly cardId: string
}

export interface ResultItem {
  /** Depends on the unit (card / art / printing id); W3 only produces card ids. */
  readonly key: string
  readonly printingId: string
}

/** The printing a card opens as: the edition's default, else the other region's, else the first. */
function representativePrinting(entry: SearchEntry, edition: Region): string | undefined {
  return (
    entry.defaultPrinting[edition] ??
    entry.defaultPrinting[edition === "jp" ? "en" : "jp"] ??
    entry.printings[0]?.id
  )
}

function textMatch(
  names: readonly SearchName[],
  flat: string,
): { readonly rank: 2 | 3; readonly index: number } | null {
  let contains = -1
  for (const [index, name] of names.entries()) {
    if (name.normalized === "") continue
    if (name.normalized.startsWith(flat)) return { rank: 2, index }
    if (contains === -1 && name.normalized.includes(flat)) contains = index
  }
  return contains === -1 ? null : { rank: 3, index: contains }
}

/** How one card matches the typed text, or null; the text must already be non-empty. */
export function matchEntry(text: string, entry: SearchEntry, options: SearchOptions): Match | null {
  const flat = normalizeText(text)
  if (flat === "") return null
  const typedKey = cardNoLookupKey(text, options.sets)
  if (typedKey !== null) {
    const exact = entry.printings.find((printing) => printing.lookupKey === typedKey)
    if (exact) return { field: "cardNo", rank: 0, printingId: exact.id }
  }
  // Digits alone (`51`, `051`) are the number part of any set (design: "part of the number");
  // a whole-number hit counts as an exact one, a shorter run of digits as a prefix.
  const digits = /^\d+$/u.test(flat) ? String(Number(flat)) : null
  if (digits !== null) {
    const whole = entry.printings.find(
      (printing) => printing.key !== null && printing.key.number === digits,
    )
    if (whole) return { field: "cardNo", rank: 0, printingId: whole.id }
  }
  // `bp01-5` should list BP01-050…059 too: leading zeros make a plain prefix miss, so a typed
  // number is compared by set and number digits; text without a number falls back to the flat form.
  const typed = cardNoKey(text, options.sets)
  const prefix = entry.printings.find(
    (printing) =>
      printing.flat.startsWith(flat) ||
      (typed !== null &&
        printing.key !== null &&
        printing.key.set === typed.set &&
        printing.key.number.startsWith(typed.number) &&
        typed.suffix === "") ||
      (digits !== null && printing.key !== null && printing.key.number.startsWith(digits)),
  )
  if (prefix) return { field: "cardNo", rank: 1, printingId: prefix.id }
  const representative = representativePrinting(entry, options.edition)
  if (representative === undefined) return null
  const name = textMatch(entry.names, flat)
  const alias = textMatch(entry.aliases, flat)
  if (name !== null && (alias === null || name.rank <= alias.rank))
    return { field: "name", rank: name.rank, printingId: representative }
  if (alias !== null) return { field: "alias", rank: alias.rank, printingId: representative }
  return null
}

/** Ranked suggestions for the typed text; the order is the sequence prev/next follows (§2.2). */
export function suggest(
  text: string,
  entries: readonly SearchEntry[],
  options: SearchOptions & { readonly limit: number },
): Suggestion[] {
  const matches: (Suggestion & { readonly order: number })[] = []
  for (const entry of entries) {
    const match = matchEntry(text, entry, options)
    if (match) matches.push({ ...match, cardId: entry.cardId, order: entry.order })
  }
  matches.sort((a, b) => a.rank - b.rank || a.order - b.order)
  return matches.slice(0, options.limit).map(({ order: _order, ...rest }) => rest)
}

function classMatches(entry: SearchEntry, classes: readonly string[]): boolean {
  if (classes.length === 0) return true
  return classes.includes(entry.classCode ?? NEUTRAL_CLASS)
}

const COST_CAP = 7

/** The stats of the region a printing belongs to, else the other region's. */
function statsOf(entry: SearchEntry, region: Region): FaceStats | undefined {
  return entry.faces[region] ?? entry.faces[region === "jp" ? "en" : "jp"]
}

function costMatches(stats: FaceStats | undefined, range: QueryState["cost"]): boolean {
  if (range.min === undefined && range.max === undefined) return true
  if (stats?.cost === null || stats?.cost === undefined) return false
  const cost = Math.min(stats.cost, COST_CAP)
  return (
    (range.min === undefined || cost >= range.min) && (range.max === undefined || cost <= range.max)
  )
}

function mechanicsMatch(
  entry: SearchEntry,
  region: Region,
  mechanics: QueryState["mechanics"],
): boolean {
  for (const [keywordId, wanted] of Object.entries(mechanics)) {
    const state = entry.mechanic(keywordId, region)
    if (wanted === "has" ? state !== "present" : state !== "absent") return false
  }
  return true
}

/** Printings of the entry that pass the printing-level facets (rarity, alt art). */
function eligiblePrintings(entry: SearchEntry, state: QueryState): SearchPrinting[] {
  return entry.printings.filter(
    (printing) =>
      (state.rarities.length === 0 ||
        (printing.rarity !== null && state.rarities.includes(printing.rarity))) &&
      (!state.altArtOnly || printing.variant !== "standard"),
  )
}

/** Type, cost and mechanics belong to one region's face: judged where the printing is. */
function regionFacetsMatch(
  entry: SearchEntry,
  printing: SearchPrinting,
  state: QueryState,
): boolean {
  const stats = statsOf(entry, printing.region)
  if (state.types.length > 0 && (stats === undefined || !state.types.includes(stats.typeCode)))
    return false
  if (!costMatches(stats, state.cost)) return false
  return mechanicsMatch(entry, printing.region, state.mechanics)
}

/** Among candidates, the edition's default printing if eligible, else the first in snapshot order. */
function representative(
  entry: SearchEntry,
  candidates: readonly SearchPrinting[],
  edition: Region,
): SearchPrinting | undefined {
  const preferred = entry.defaultPrinting[edition]
  return (
    candidates.find((printing) => printing.id === preferred) ??
    candidates.find((printing) => printing.region === edition) ??
    candidates[0]
  )
}

interface Sortable extends ResultItem {
  readonly order: number
  readonly rank: number
  readonly printing: SearchPrinting
  readonly stats: FaceStats | undefined
}

function compareNullable(a: number | null | undefined, b: number | null | undefined): number {
  if (a === b) return 0
  if (a === null || a === undefined) return 1
  if (b === null || b === undefined) return -1
  return a - b
}

const COMPARATORS: Record<QuerySort, (a: Sortable, b: Sortable) => number> = {
  no: (a, b) =>
    a.rank - b.rank || a.order - b.order || compareCodePoints(a.printing.cardNo, b.printing.cardNo),
  cost: (a, b) => compareNullable(a.stats?.cost, b.stats?.cost) || a.order - b.order,
  atk: (a, b) => compareNullable(a.stats?.attack, b.stats?.attack) || a.order - b.order,
  def: (a, b) => compareNullable(a.stats?.defense, b.stats?.defense) || a.order - b.order,
  name: (a, b) =>
    compareCodePoints(normalizeText(a.stats?.name ?? ""), normalizeText(b.stats?.name ?? "")) ||
    a.order - b.order,
  date: (a, b) => {
    const left = a.printing.releasedOn
    const right = b.printing.releasedOn
    if (left === right) return a.order - b.order
    if (left === null) return 1
    if (right === null) return -1
    return compareCodePoints(left, right)
  },
}

/**
 * The result list for a URL state: a pure function of the entries, so the same snapshot and the
 * same URL always give the same sequence. Every facet of the state applies; `unit` decides what
 * one cell is (a card, an art, a printing) and the sort orders the cells.
 */
export function apply(
  state: QueryState,
  entries: readonly SearchEntry[],
  options: SearchOptions,
): ResultItem[] {
  const items: Sortable[] = []
  for (const entry of entries) {
    if (!classMatches(entry, state.classes)) continue
    if (state.sets.length > 0 && !state.sets.includes(entry.setCode)) continue
    const match = state.text === "" ? null : matchEntry(state.text, entry, options)
    if (state.text !== "" && match === null) continue
    // Region-bound facets are judged per printing first, so the representative of a card or an
    // art group is chosen among the printings that pass: a card costing 2 in JP and 3 in EN stays
    // listed, as its EN printing, when cost 3 is asked for.
    const candidates = eligiblePrintings(entry, state).filter((printing) =>
      regionFacetsMatch(entry, printing, state),
    )
    if (candidates.length === 0) continue
    const push = (printing: SearchPrinting, key: string) => {
      items.push({
        key,
        printingId: printing.id,
        order: entry.order,
        rank: match?.rank ?? 0,
        printing,
        stats: statsOf(entry, printing.region),
      })
    }
    if (state.unit === "printing") {
      for (const printing of candidates) push(printing, printing.id)
    } else if (state.unit === "art") {
      const groups = new Map<string, SearchPrinting[]>()
      for (const printing of candidates) {
        const key = printing.artId ?? printing.id
        groups.set(key, [...(groups.get(key) ?? []), printing])
      }
      for (const [key, group] of groups) {
        const chosen = representative(entry, group, options.edition)
        if (chosen) push(chosen, key)
      }
    } else {
      // A typed card number opens that very printing; otherwise the edition's default.
      const typed =
        match?.field === "cardNo" ? candidates.find((p) => p.id === match.printingId) : undefined
      const chosen = typed ?? representative(entry, candidates, options.edition)
      if (chosen) push(chosen, entry.cardId)
    }
  }
  items.sort(COMPARATORS[state.sort])
  return items.map(({ key, printingId }) => ({ key, printingId }))
}
