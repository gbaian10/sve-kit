import { useMemo } from "react"

import { LIST_PAGE_SIZE } from "../../app/listEntryState"
import { useNeighbours, useWorkerSuggestions } from "../../app/snapshot"
import type { CardSummary, Row } from "../../data"
import { parseQuery } from "../../domain/query/codec"
import type { QueryState } from "../../domain/query/model"
import type { Region } from "../../domain/search"

interface SequenceItem {
  readonly key: string
  readonly printingId: string
  readonly summary?: CardSummary | undefined
}

interface Neighbour {
  readonly key: string
  readonly summary: CardSummary
}

// Distinguish a direct link from a card missing from its source list (architecture §2.2).
type CardSequence =
  | { readonly phase: "none" | "pending" | "missing" }
  | { readonly phase: "found"; readonly previous?: Neighbour; readonly next?: Neighbour }

interface SequenceCard {
  readonly cardId: string
  readonly printingId: string
  readonly artId?: string | undefined
}

// Match the search index's art grouping, which uses only the front face.
export function frontArtOf(printing: Row | undefined): string | undefined {
  const faces = printing?.["faces"]
  const first = Array.isArray(faces) ? faces[0] : undefined
  const art =
    typeof first === "object" && first !== null && !Array.isArray(first)
      ? first["art_id"]
      : undefined
  return typeof art === "string" && art !== "" ? art : undefined
}

function unitKeyOf(unit: QueryState["unit"], card: SequenceCard): string {
  if (unit === "card") return card.cardId
  if (unit === "art") return card.artId ?? card.printingId
  return card.printingId
}

function locate(
  order: readonly SequenceItem[],
  targets: readonly (string | null | undefined)[],
): number {
  for (const target of targets) {
    if (target === null || target === undefined) continue
    const position = order.findIndex((item) => item.key === target || item.printingId === target)
    if (position >= 0) return position
  }
  return -1
}

function neighbourAt(order: readonly SequenceItem[], position: number): Neighbour | undefined {
  const item = order[position]
  return item?.summary ? { key: item.key, summary: item.summary } : undefined
}

function found(order: readonly SequenceItem[], position: number): CardSequence {
  if (position < 0) return { phase: "missing" }
  const previous = neighbourAt(order, position - 1)
  const next = neighbourAt(order, position + 1)
  return {
    phase: "found",
    ...(previous ? { previous } : {}),
    ...(next ? { next } : {}),
  }
}

// Rebuild the source sequence from its URL rather than retained list state (architecture §2.2).
export function useCardSequence(
  entry: Readonly<Record<string, unknown>> | null,
  edition: Region,
  printingId: string | null,
  cardId: string | undefined,
  artId: string | undefined,
): CardSequence {
  // Plain values, not an object built here: the React Compiler keeps the caller's memoization.
  const current: SequenceCard | undefined =
    printingId !== null && cardId !== undefined ? { cardId, printingId, artId } : undefined
  const background = typeof entry?.["background"] === "string" ? entry["background"] : null
  const fromSuggest = entry?.["source"] === "suggest"
  const pages = typeof entry?.["pages"] === "number" ? Math.max(1, entry["pages"]) : 1
  const query = useMemo(
    () =>
      background === null ? null : { state: parseQuery(new URLSearchParams(background)), edition },
    [background, edition],
  )
  const hint = entry?.["route"] as { printingId?: unknown } | undefined
  const resultKey =
    typeof entry?.["resultKey"] === "string"
      ? entry["resultKey"]
      : typeof hint?.printingId === "string"
        ? hint.printingId
        : (current?.printingId ?? null)
  const unit = fromSuggest ? "card" : (query?.state.unit ?? "printing")
  const unitKey = current ? unitKeyOf(unit, current) : null

  const resultsQuery = fromSuggest ? null : query
  const maxItems = pages * LIST_PAGE_SIZE
  const primary = useNeighbours(resultsQuery, resultsQuery ? resultKey : null, maxItems)
  const primaryOrder =
    resultsQuery && primary.result.phase === "complete" ? primary.result.page.items : undefined
  const primaryAt = primaryOrder ? locate(primaryOrder, [resultKey]) : -1
  // The list's own key names no cell: ask again for the shown printing's cell.
  const fallbackTarget =
    primaryOrder && primaryAt < 0 && unitKey !== null && unitKey !== resultKey ? unitKey : null
  const fallback = useNeighbours(fallbackTarget ? resultsQuery : null, fallbackTarget, maxItems)

  // Suggestions are recomputed for the background's text; only an answer for that same text and
  // edition counts, so a late answer for an older text never places the card.
  const text = fromSuggest ? (query?.state.text ?? "") : ""
  const suggestions = useWorkerSuggestions(text, edition)

  if (query === null) return { phase: "none" }
  if (fromSuggest) {
    const answer = suggestions.result
    if (
      answer.phase !== "complete" ||
      suggestions.lastInput?.state.text !== text ||
      suggestions.lastInput.edition !== edition
    )
      return answer.phase === "error" ? { phase: "missing" } : { phase: "pending" }
    const order: SequenceItem[] = (answer.suggestions ?? []).map(({ summary }) => ({
      key: summary.cardId,
      printingId: summary.printingId,
      summary,
    }))
    return found(order, locate(order, [resultKey, unitKey]))
  }
  if (primary.result.phase === "error") return { phase: "missing" }
  if (!primaryOrder) return { phase: "pending" }
  if (primaryAt >= 0) return found(primaryOrder, primaryAt)
  if (fallbackTarget === null) return { phase: "missing" }
  if (fallback.result.phase === "error") return { phase: "missing" }
  if (fallback.result.phase !== "complete") return { phase: "pending" }
  const order = fallback.result.page.items
  return found(order, locate(order, [fallbackTarget]))
}
