import type { Region } from "../domain/search"

/** A deck's builder page; an EN deck says so in `ed`, so its edition loads from the start. */
export function deckPath(deck: { readonly id: string; readonly edition: Region }): string {
  return `/decks/${encodeURIComponent(deck.id)}${deck.edition === "en" ? "?ed=en" : ""}`
}

/** `/decks/<id>` is the builder; the list (`/decks`) is not. */
export function isDeckBuilderPath(pathname: string): boolean {
  return /^\/decks\/[^/]+$/u.test(pathname)
}
