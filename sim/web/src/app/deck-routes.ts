import type { Region } from "../domain/search"

/** A deck's builder page; an EN deck says so in `ed`, so its edition loads from the start. */
export function deckPath(deck: { readonly id: string; readonly edition: Region }): string {
  return `/decks/${encodeURIComponent(deck.id)}${deck.edition === "en" ? "?ed=en" : ""}`
}

/** `/decks/<id>` is the builder; the list (`/decks`) and the import page are not. */
export function isDeckBuilderPath(pathname: string): boolean {
  return /^\/decks\/(?!import$)[^/]+$/u.test(pathname)
}

/** The import page opened on a share code or a deck list (`d`), as the share links use. */
export function deckImportPath(deck: string): string {
  return `/decks/import?d=${encodeURIComponent(deck)}`
}

/** A link that opens the import page on this share code from anywhere. */
export function deckShareLink(code: string): string {
  return `${window.location.origin}${deckImportPath(code)}`
}
