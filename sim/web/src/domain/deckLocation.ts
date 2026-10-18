// The deck-location row, in the maintainer's order. Evolve, advance and Token come from the data's
// special kinds; leader, EP and SEP from the type; "main" is every printing in none of the others.
// Token and leader keep the sub-values they had under the former type filter (`token:<type>`,
// `leader:<frame>`), so the tokens older `type=` URLs carry still resolve.
export const LOCATION_ORDER = ["main", "evolve", "advance", "leader", "token", "ep", "sep"] as const
const TOKEN_TYPES = ["follower", "spell", "amulet", "crest", "equipment"] as const
const LEADER_FRAMES = ["gold", "white", "unknown"] as const

/** Whether a filter token names a deck location (or one of its sub-values) rather than a type. */
export function isLocationToken(token: string): boolean {
  const [parent = ""] = token.split(":")
  return (LOCATION_ORDER as readonly string[]).includes(parent)
}

/** The Token kind tokens (`token:<type>`) with printings, in display order. */
export function tokenKindTokens(counts: Readonly<Record<string, number>>): string[] {
  return TOKEN_TYPES.map((kind) => `token:${kind}`).filter((token) => (counts[token] ?? 0) > 0)
}

/** The leader frame tokens (`leader:<frame>`) with printings, in display order. */
export function leaderFrameTokens(counts: Readonly<Record<string, number>>): string[] {
  return LEADER_FRAMES.map((frame) => `leader:${frame}`).filter((token) => (counts[token] ?? 0) > 0)
}

const rank = (order: readonly string[], code: string) => {
  const index = order.indexOf(code)
  return index === -1 ? order.length : index
}

/** Selected location tokens (`main`, `token:spell`, `leader:gold`) in the row's display order. */
export function orderLocationTokens(tokens: readonly string[]): string[] {
  const key = (token: string): [number, number] => {
    const [parent = "", child] = token.split(":")
    const children: readonly string[] = parent === "leader" ? LEADER_FRAMES : TOKEN_TYPES
    return [rank(LOCATION_ORDER, parent), child === undefined ? -1 : rank(children, child)]
  }
  return [...tokens].sort((a, b) => {
    const [pa, ca] = key(a)
    const [pb, cb] = key(b)
    return pa - pb || ca - cb
  })
}
