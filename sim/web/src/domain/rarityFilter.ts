import { BADGE_STYLES } from "./rarityBadges"
import { PROMO_RARITY, RARITY_LADDER } from "./representative"

export const UNRATED_RARITY = "unrated"
export const PREMIUM_RARITY = "premium"

// The rarity ladder runs low to high, then Premium and PR.
const RARITY_ORDER: readonly string[] = [...RARITY_LADDER, PREMIUM_RARITY, PROMO_RARITY]

/** Applies official rarity tokens and the shared premium dimension to one printing. */
export function matchesRaritySelection(
  rarityCode: string | null,
  premium: boolean,
  selected: readonly string[],
  badgeStyle?: string,
): boolean {
  if (selected.length === 0) return true
  const premiumOnly = selected.includes(PREMIUM_RARITY)
  if (premium !== premiumOnly) return false
  const raritySelections = selected.filter((token) => token !== PREMIUM_RARITY)
  if (raritySelections.length === 0) return true
  const rarity = rarityCode ?? ""
  return raritySelections.some((token) => {
    if (token === UNRATED_RARITY) return rarity === "" || rarity === "-"
    if (token.startsWith("pr:")) return rarity === "pr" && token === `pr:${badgeStyle ?? "unknown"}`
    return token === rarity
  })
}

/** Vocabulary rarities in chip order; codes the order does not know keep their own order after it. */
export function orderRarities<T extends { readonly code: string }>(rarities: readonly T[]): T[] {
  const rank = (code: string) => {
    const index = RARITY_ORDER.indexOf(code)
    return index === -1 ? RARITY_ORDER.length : index
  }
  return [...rarities].sort((a, b) => rank(a.code) - rank(b.code))
}

/** The PR appearance tokens (`pr:<style>`) the panel offers, in badge order. */
export function prStyleTokens(styles: readonly string[]): string[] {
  return [...BADGE_STYLES, "unknown"]
    .filter((style) => styles.includes(style))
    .map((style) => `pr:${style}`)
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
