/**
 * The artist selection for printings the official site gives no credit. A reserved word rather
 * than an artist id: ids always read `y:<32 hex>`, so no artist can ever collide with it.
 */
export const UNCREDITED_ARTIST = "uncredited"

/** Selected artist tokens in the filter's display order: the uncredited choice sits last. */
export function orderArtistTokens(tokens: readonly string[]): string[] {
  return [...tokens].sort(
    (a, b) => Number(a === UNCREDITED_ARTIST) - Number(b === UNCREDITED_ARTIST),
  )
}
