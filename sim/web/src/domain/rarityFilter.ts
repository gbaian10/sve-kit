import { BADGE_STYLES } from "./rarityBadges"

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

/** Selected rarity tokens (`lg`, `pr`, `pr:gr`, `unrated`) in the filter's display order. */
export function orderRarityTokens(tokens: readonly string[]): string[] {
  const key = (token: string): [number, number] => {
    if (token === UNRATED_RARITY) return [RARITY_ORDER.length + 1, 0]
    const [code = "", style] = token.split(":")
    const index = RARITY_ORDER.indexOf(code)
    const styles: readonly string[] = [...BADGE_STYLES, "unknown"]
    return [
      index === -1 ? RARITY_ORDER.length : index,
      style === undefined ? -1 : styles.indexOf(style),
    ]
  }
  return [...tokens].sort((a, b) => {
    const [pa, ca] = key(a)
    const [pb, cb] = key(b)
    return pa - pb || ca - cb
  })
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
