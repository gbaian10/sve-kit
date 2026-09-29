import { type CardNoKey, cardNoKey, cardNoLookupKey } from "./cardNo"
import { normalizeText } from "./normalize"
import { NEUTRAL_CLASS, type QueryState } from "./query/model"

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
  // `bp01-5` should list BP01-050…059 too: leading zeros make a plain prefix miss, so a typed
  // number is compared by set and number digits; text without a number falls back to the flat form.
  const typed = cardNoKey(text, options.sets)
  // Digits alone (`51`, `051`) are the number part of any set (design: "part of the number").
  const digits = /^\d+$/u.test(flat) ? String(Number(flat)) : null
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
