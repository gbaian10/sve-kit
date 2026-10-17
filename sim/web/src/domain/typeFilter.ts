// The type filter's chip order (the maintainer's), and the sub-values behind Token and leader.
export const TYPE_ORDER = ["follower", "spell", "amulet", "token", "leader", "ep", "sep"] as const
export const TOKEN_TYPES = ["follower", "spell", "amulet", "crest", "equipment"] as const
export const LEADER_FRAMES = ["gold", "white", "unknown"] as const

const rank = (order: readonly string[], code: string) => {
  const index = order.indexOf(code)
  return index === -1 ? order.length : index
}

/** Selected type tokens (`token`, `token:spell`, `leader:gold`) in the filter's display order. */
export function orderTypeTokens(tokens: readonly string[]): string[] {
  const key = (token: string): [number, number] => {
    const [parent = "", child] = token.split(":")
    const children: readonly string[] = parent === "leader" ? LEADER_FRAMES : TOKEN_TYPES
    return [rank(TYPE_ORDER, parent), child === undefined ? -1 : rank(children, child)]
  }
  return [...tokens].sort((a, b) => {
    const [pa, ca] = key(a)
    const [pb, cb] = key(b)
    return pa - pb || ca - cb
  })
}
