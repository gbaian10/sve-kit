import { type CardSummary, type CompleteView, type ImageIndex, viewImages } from "../../data"
import { printingPath } from "../../domain/cardRoute"
import type { DisplayedText, NameSource } from "../../domain/nameDisplay"
import { DEFAULT_QUERY } from "../../domain/query/model"
import type { Region, TextLang } from "../../domain/search"

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

/** A set name in the interface's text language, else the original. */
export function setLabel(name: NameSource, lang: TextLang): DisplayedText {
  const translated = name.translations[lang]
  return translated ? { text: translated, lang } : name.original
}

/** Thumbnails of the faces a suggestion or recent answer brought along. */
export function faceImages(faces: CompleteView["faces"], edition: Region): ImageIndex {
  return viewImages({
    query: { state: DEFAULT_QUERY, edition },
    page: { items: [], total: 0, offset: 0 },
    options: { classes: [], sets: [] },
    faces,
  })
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
