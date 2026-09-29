// Search normalization for user input and card names: NFKC (fullwidth/halfwidth, compatibility
// forms), case folding, and no whitespace or hyphen differences. The exporter's
// `search_alias.normalized` must follow the same rules (config.search.normalizer_version), so this
// module is versioned by that value.
export const NORMALIZER_VERSION = "synthetic-v1"

const SEPARATORS = /[\s\u3000\-\u2010-\u2015_]+/gu

/** Text with case, width and separator differences removed; empty input stays empty. */
export function normalizeText(text: string): string {
  return text.normalize("NFKC").toLowerCase().replace(SEPARATORS, "")
}

/** Like `normalizeText` but keeps single spaces between words, for display of what matched. */
export function normalizeLoose(text: string): string {
  return text.normalize("NFKC").toLowerCase().replace(SEPARATORS, " ").trim()
}

/** Orders strings by Unicode code point, matching the snapshot's canonical sort (build-db §14). */
export function compareCodePoints(a: string, b: string): number {
  const left = Array.from(a)
  const right = Array.from(b)
  const length = Math.min(left.length, right.length)
  for (let i = 0; i < length; i += 1) {
    const diff = (left[i]?.codePointAt(0) ?? 0) - (right[i]?.codePointAt(0) ?? 0)
    if (diff !== 0) return diff
  }
  return left.length - right.length
}
