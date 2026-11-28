import { BADGE_STYLES } from "./rarityBadges"
import { PROMO_RARITY, RARITY_LADDER } from "./representative"

export const UNRATED_RARITY = "unrated"
export const PREMIUM_RARITY = "premium"

// The rarity ladder runs low to high, then Premium and PR.
const RARITY_ORDER: readonly string[] = [...RARITY_LADDER, PREMIUM_RARITY, PROMO_RARITY]
// PR appearances follow the same ladder, then the two that have no badge.
const PR_STYLE_ORDER: readonly string[] = [
  ...RARITY_LADDER.filter((code) => (BADGE_STYLES as readonly string[]).includes(code)),
  "none",
  "unknown",
]

// PR pack leaders can be found with other promos even though their cards carry no rarity.
export const PR_PACK = "pr"

/** What the rarity filter reads from one printing. */
interface RarityFilterInput {
  readonly rarityCode: string | null
  readonly premium: boolean
  /** The PR appearance style (`rarity_badge.style`); absent means not recorded. */
  readonly badgeStyle?: string | undefined
  /** The family code of the printing's home set. */
  readonly owner?: string | undefined
  readonly type?: string | undefined
}

/** The rarity token a printing is filed under, plus its PR appearance when that token is PR. */
export function filterRarity({
  rarityCode,
  badgeStyle,
  owner,
  type,
}: Omit<RarityFilterInput, "premium">): { readonly rarity: string; readonly style?: string } {
  const rarity = rarityCode ?? ""
  if (rarity === "" || rarity === "-")
    return owner === PR_PACK && type === "leader"
      ? { rarity: PROMO_RARITY, style: "none" }
      : { rarity: UNRATED_RARITY }
  return rarity === PROMO_RARITY ? { rarity, style: badgeStyle ?? "unknown" } : { rarity }
}

/** Applies official rarity tokens and the shared premium dimension to one printing. */
export function matchesRaritySelection(
  printing: RarityFilterInput,
  selected: readonly string[],
): boolean {
  if (selected.length === 0) return true
  const premiumOnly = selected.includes(PREMIUM_RARITY)
  if (printing.premium !== premiumOnly) return false
  const raritySelections = selected.filter((token) => token !== PREMIUM_RARITY)
  if (raritySelections.length === 0) return true
  const { rarity, style } = filterRarity(printing)
  return raritySelections.some((token) =>
    token.startsWith("pr:") ? style !== undefined && token === `pr:${style}` : token === rarity,
  )
}

/** Vocabulary rarities in chip order; codes the order does not know keep their own order after it. */
export function orderRarities<T extends { readonly code: string }>(rarities: readonly T[]): T[] {
  const rank = (code: string) => {
    const index = RARITY_ORDER.indexOf(code)
    return index === -1 ? RARITY_ORDER.length : index
  }
  return rarities.toSorted((a, b) => rank(a.code) - rank(b.code))
}

/** The PR appearance tokens (`pr:<style>`) the panel offers, in ladder order. */
export function prStyleTokens(styles: readonly string[]): string[] {
  return PR_STYLE_ORDER.filter((style) => styles.includes(style)).map((style) => `pr:${style}`)
}

/** Selected rarity tokens (`lg`, `pr`, `pr:gr`, `unrated`) in the filter's display order. */
export function orderRarityTokens(tokens: readonly string[]): string[] {
  const key = (token: string): [number, number] => {
    if (token === UNRATED_RARITY) return [RARITY_ORDER.length + 1, 0]
    const [code = "", style] = token.split(":")
    const index = RARITY_ORDER.indexOf(code)
    return [
      index === -1 ? RARITY_ORDER.length : index,
      style === undefined ? -1 : PR_STYLE_ORDER.indexOf(style),
    ]
  }
  return tokens.toSorted((a, b) => {
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
