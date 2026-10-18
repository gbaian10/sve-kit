// Which printing stands for a merged cell (one per card or one per art): the maintainer's rule
// from #781. The rarity ladder is the one place this order is written; PR sits off the ladder.
export const RARITY_LADDER = ["br", "sr", "gr", "lg", "sl", "ur", "sp", "ssp"] as const
export const PROMO_RARITY = "pr"
/** A printing whose metadata has not arrived yet (a catalog locator), as opposed to one with none. */
export const UNKNOWN_RARITY = "?"

export const REPRESENTATIVE_RARITIES = ["low", "high"] as const
export type RepresentativeRarity = (typeof REPRESENTATIVE_RARITIES)[number]

export interface RepresentativeChoice {
  /** Which end of the ladder the representative comes from, among the non-PR printings. */
  readonly rarity: RepresentativeRarity
  /** A PR printing wins whenever the group has one. */
  readonly prFirst: boolean
}

/** Ladder position of a rarity code; anything off the ladder (no rarity, "-", PR) sits below BR. */
export function rarityRank(code: string | null | undefined): number {
  return (RARITY_LADDER as readonly string[]).indexOf(code ?? "")
}

/**
 * The representative of one group of printings that all passed the filters: a PR when asked for
 * and present, else the lowest or highest ladder rank among the non-PR printings; a group of PR
 * printings only shows its PR. Ties, and PR against PR, go to the earliest card number. Printings
 * whose rarity is still unknown stand aside while any known one is in the group, so a cell does
 * not flip to a stray reprint before its metadata lands.
 */
export function pickRepresentative<T>(
  candidates: readonly T[],
  rarityOf: (candidate: T) => string,
  orderOf: (candidate: T) => number,
  choice: RepresentativeChoice,
): T | undefined {
  const known = candidates.filter((candidate) => rarityOf(candidate) !== UNKNOWN_RARITY)
  const pool0 = known.length > 0 ? known : candidates
  const promos = pool0.filter((candidate) => rarityOf(candidate) === PROMO_RARITY)
  const others = pool0.filter((candidate) => rarityOf(candidate) !== PROMO_RARITY)
  const pool = (choice.prFirst && promos.length > 0) || others.length === 0 ? promos : others
  let best: T | undefined
  let bestRank = 0
  let bestOrder = 0
  for (const candidate of pool) {
    const rank = rarityRank(rarityOf(candidate))
    const order = orderOf(candidate)
    const better =
      best === undefined ||
      (choice.rarity === "high" ? rank > bestRank : rank < bestRank) ||
      (rank === bestRank && order < bestOrder)
    if (better) {
      best = candidate
      bestRank = rank
      bestOrder = order
    }
  }
  return best
}
