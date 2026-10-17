export const UNRATED_RARITY = "unrated"

// The maintainer's chip order: the rarity ladder low to high, PR after the printed rarities.
const RARITY_ORDER = ["br", "sr", "gr", "lg", "sl", "ur", "sp", "ssp", "pr"]

/** Vocabulary rarities in chip order; codes the order does not know keep their own order after it. */
export function orderRarities<T extends { readonly code: string }>(rarities: readonly T[]): T[] {
  const rank = (code: string) => {
    const index = RARITY_ORDER.indexOf(code)
    return index === -1 ? RARITY_ORDER.length : index
  }
  return [...rarities].sort((a, b) => rank(a.code) - rank(b.code))
}

export function toggleFacet(
  selected: readonly string[],
  token: string,
  children: readonly string[],
  parent: string,
): string[] {
  if (token === parent) {
    const others = selected.filter((value) => value !== parent && !value.startsWith(`${parent}:`))
    const all =
      selected.includes(parent) ||
      (children.length > 0 && children.every((value) => selected.includes(value)))
    return all ? others : [...others, parent].sort()
  }
  const next = new Set(selected)
  if (token.startsWith(`${parent}:`) && next.delete(parent))
    for (const child of children) next.add(child)
  if (!next.delete(token)) next.add(token)
  if (children.length > 0 && children.every((child) => next.has(child))) {
    for (const value of next) if (value.startsWith(`${parent}:`)) next.delete(value)
    next.add(parent)
  }
  return [...next].sort()
}
