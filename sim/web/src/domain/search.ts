import type { CardNoKey } from "./cardNo"
import { NEUTRAL_CLASS, type QueryState } from "./query/model"
import { type Match, matchEntry } from "./search-match"
import { representativePrinting } from "./search-match"

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

export interface SearchPrinting {
  readonly id: string
  readonly region: Region
  readonly cardNo: string
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
}

export type MatchedField = "cardNo" | "name" | "alias" | "effect"

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

/**
 * The result list for a URL state: a pure function of the entries, so the same snapshot and the
 * same URL always give the same sequence. W3 applies `text` and `classes`; the other facets and
 * units arrive with the filter panel.
 */
export function apply(
  state: QueryState,
  entries: readonly SearchEntry[],
  options: SearchOptions,
): ResultItem[] {
  // With text, the order is the suggest list's (rank, then snapshot order), so "see all" and the
  // suggestions agree; without text it is the snapshot order.
  const items: (ResultItem & { readonly rank: number; readonly order: number })[] = []
  for (const entry of entries) {
    if (!classMatches(entry, state.classes)) continue
    const match = state.text === "" ? null : matchEntry(state.text, entry, options)
    if (state.text !== "" && match === null) continue
    const printingId = match?.printingId ?? representativePrinting(entry, options.edition)
    if (printingId === undefined) continue
    items.push({ key: entry.cardId, printingId, rank: match?.rank ?? 0, order: entry.order })
  }
  items.sort((a, b) => a.rank - b.rank || a.order - b.order)
  return items.map(({ key, printingId }) => ({ key, printingId }))
}
