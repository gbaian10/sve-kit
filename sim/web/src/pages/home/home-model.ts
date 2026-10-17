import { type CardSummary } from "../../data"
import { printingPath } from "../../domain/cardRoute"
import type { DisplayedText } from "../../domain/nameDisplay"
import type { Region } from "../../domain/search"

export const RECENT_SET_LIMIT = 3

export interface SetEntry {
  /** `product_family.code`, the `set=` value. */
  readonly code: string
  readonly publicCode: string
  readonly name: DisplayedText
  readonly releasedOn: string
}

export function cardPath(summary: CardSummary): string {
  return printingPath(summary)
}

export function setPath(publicCode: string, edition: Region = "jp"): string {
  return `/sets/${encodeURIComponent(publicCode)}${edition === "en" ? "?ed=en" : ""}`
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
