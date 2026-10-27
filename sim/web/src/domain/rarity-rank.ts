export const RARITY_LADDER = ["br", "sr", "gr", "lg", "sl", "ur", "sp", "ssp"] as const

/** Ladder position of a rarity code; anything off the ladder (no rarity, "-", PR) sits below BR. */
export function rarityRank(code: string | null | undefined): number {
  return (RARITY_LADDER as readonly string[]).indexOf(code ?? "")
}
