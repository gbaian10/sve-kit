/**
 * Reserved artist selections. They are words rather than artist ids: ids always read
 * `y:<32 hex>`, so no artist can ever collide with them.
 * - `uncredited`: printings the official site gives no credit.
 * - `credited`: every credited printing, i.e. all artists at once, without listing their ids.
 */
export const UNCREDITED_ARTIST = "uncredited"
export const CREDITED_ARTIST = "credited"

const RESERVED = [CREDITED_ARTIST, UNCREDITED_ARTIST]
const idsOf = (selected: readonly string[]) => selected.filter((token) => !RESERVED.includes(token))

/** Selected artist tokens in the filter's display order: "credited" first, "uncredited" last. */
export function orderArtistTokens(tokens: readonly string[]): string[] {
  const rank = (token: string) =>
    token === CREDITED_ARTIST ? 0 : token === UNCREDITED_ARTIST ? 2 : 1
  return [...tokens].sort((a, b) => rank(a) - rank(b))
}

/** Whether one artist reads as selected: by itself, or because every artist is selected. */
export function artistChecked(selected: readonly string[], id: string): boolean {
  return selected.includes(CREDITED_ARTIST) || selected.includes(id)
}

/** The "credited" row: all artists selected, some of them, or none. */
export function creditedState(
  selected: readonly string[],
  allIds: readonly string[],
): "all" | "some" | "none" {
  if (selected.includes(CREDITED_ARTIST)) return "all"
  const chosen = idsOf(selected).filter((id) => allIds.includes(id))
  if (allIds.length > 0 && chosen.length === allIds.length) return "all"
  return chosen.length > 0 ? "some" : "none"
}

/** Everything chosen collapses to the "credited" word; the uncredited choice is kept as it is. */
function collapse(selected: readonly string[], allIds: readonly string[]): string[] {
  const keepUncredited = selected.includes(UNCREDITED_ARTIST) ? [UNCREDITED_ARTIST] : []
  const ids = idsOf(selected)
  return allIds.length > 0 && allIds.every((id) => ids.includes(id))
    ? [CREDITED_ARTIST, ...keepUncredited]
    : [...selected]
}

/** Ticking "credited" selects every artist, unticking it clears them all; "uncredited" stays. */
export function toggleCredited(selected: readonly string[], allIds: readonly string[]): string[] {
  const keepUncredited = selected.includes(UNCREDITED_ARTIST) ? [UNCREDITED_ARTIST] : []
  return creditedState(selected, allIds) === "all"
    ? keepUncredited
    : [CREDITED_ARTIST, ...keepUncredited]
}

/** One artist toggled; from "all artists" the rest are listed, and a full list collapses again. */
export function toggleArtist(
  selected: readonly string[],
  id: string,
  allIds: readonly string[],
): string[] {
  const keepUncredited = selected.includes(UNCREDITED_ARTIST) ? [UNCREDITED_ARTIST] : []
  const base =
    creditedState(selected, allIds) === "all" ? [...allIds, ...keepUncredited] : [...selected]
  const next = base.includes(id) ? base.filter((token) => token !== id) : [...base, id]
  return collapse(
    next.filter((token) => token !== CREDITED_ARTIST),
    allIds,
  )
}
