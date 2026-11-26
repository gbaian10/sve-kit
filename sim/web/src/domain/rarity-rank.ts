// The card database's order (`registry/rarity.py`), repeated here because the snapshot does not
// carry it. Chip order, printing rows, PR appearance order and the representative all read this.
export const RARITY_LADDER = ["br", "sr", "gr", "lg", "sl", "gbf", "ur", "sp", "ssp"] as const

/** Ladder position of a rarity code; anything off the ladder (no rarity, "-", PR) sits below BR. */
export function rarityRank(code: string | null | undefined): number {
  return (RARITY_LADDER as readonly string[]).indexOf(code ?? "")
}
