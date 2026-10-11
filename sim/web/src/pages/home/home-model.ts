import type { CardSummary } from "../../data"
import type { DisplayedText } from "../../domain/nameDisplay"

export const RECENT_SET_LIMIT = 3

export interface SetEntry {
  /** `product_family.code`, the `set=` value. */
  readonly code: string
  readonly publicCode: string
  readonly name: DisplayedText | undefined
  readonly releasedOn: string
}

export function cardPath(summary: CardSummary): string {
  return `/cards/${encodeURIComponent(summary.cardNo)}`
}

export function setPath(publicCode: string): string {
  return `/sets/${encodeURIComponent(publicCode)}`
}

/** What the home page keeps on its own history entry, so back restores the typed text. */
interface HomeEntry {
  readonly text: string
}

export function homeEntryState(text: string): HomeEntry {
  return { text }
}

export function readHomeEntry(state: unknown): HomeEntry {
  const text =
    typeof state === "object" && state !== null ? (state as Record<string, unknown>)["text"] : null
  return { text: typeof text === "string" ? text : "" }
}
