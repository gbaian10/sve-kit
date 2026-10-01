import type { Catalog } from "../data"
import { parseQuery } from "../domain/query/codec"
import type { Region, ResultItem } from "../domain/search"
import type { CardEntryState } from "./listEntryState"

const PAGE_SIZE = 60
const SUGGEST_LIMIT = 8

/**
 * The sequence prev/next walks (architecture §2.2): the list's results for the background search
 * string, limited to the pages it had loaded; for a suggest-opened card the suggestions of that
 * text, keyed by card id like the list.
 */
export function sequenceFor(
  state: CardEntryState,
  catalog: Catalog,
  edition: Region,
): readonly ResultItem[] {
  const params = new URLSearchParams(state.background)
  const query = parseQuery(params)
  if (state.source === "suggest") {
    return catalog
      .suggest(query.text, edition, SUGGEST_LIMIT)
      .map((item) => ({ key: item.cardId, printingId: item.printingId }))
  }
  return catalog.results(query, edition).slice(0, Math.max(1, state.pages) * PAGE_SIZE)
}

export interface Neighbours {
  readonly prev: ResultItem | undefined
  readonly next: ResultItem | undefined
  /** False when the current card is not in the sequence at all (snapshot changed, filters differ). */
  readonly inSequence: boolean
}

/** Finds the current position by result key, else by the card the printing belongs to. */
export function neighbours(
  sequence: readonly ResultItem[],
  resultKey: string,
  cardId: string,
): Neighbours {
  let index = sequence.findIndex((item) => item.key === resultKey)
  if (index === -1) index = sequence.findIndex((item) => item.key === cardId)
  if (index === -1) return { prev: undefined, next: undefined, inSequence: false }
  return { prev: sequence[index - 1], next: sequence[index + 1], inSequence: true }
}
