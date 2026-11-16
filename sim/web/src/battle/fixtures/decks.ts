// The leaders and evolve decks the fixture scenes are built from.
import type { CardNo } from "./cards"

export const DRAGON_LEADER: CardNo = "BP10-LD05"
export const ROYAL_LEADER: CardNo = "BP05-LD02"
export const WITCH_LEADER: CardNo = "BP01-LD05"

export const DRAGON_EVOLVE: readonly CardNo[] = [
  "BP05-053",
  "BP05-053",
  "BP05-053",
  "BP20-065",
  "BP20-065",
  "BP15-114",
  "BP15-114",
  "BP15-067",
  "BP20-058",
  "BP05-056",
]
export const WITCH_EVOLVE: readonly CardNo[] = [
  "BP01-053",
  "BP01-053",
  "BP01-055",
  "BP01-055",
  "BP01-069",
  "BP01-069",
]
export const ROYAL_EVOLVE: readonly CardNo[] = [
  "BP16-032",
  "BP16-032",
  "BP16-032",
  "BP13-027",
  "BP13-027",
  "BP13-027",
  "BP13-031",
  "BP13-031",
  "BP13-031",
  "BP13-020",
]

// When each seat may first super evolve, counted in its own turns, as the authority hands it out.
export const SEP_FROM = { first: 7, second: 6 }
