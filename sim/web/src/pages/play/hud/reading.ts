// How the info column, the selection bar and the large view word what a card is, costs and hits
// for. Where the authority says a current value that is the one shown; what the card prints is
// said to be printed, and a value the authority does not give is said to be unknown. A printed
// value never stands in for a current one.
import type { Inspection } from "../../../battle/view/inspect"

export type Kind = "leader" | "follower" | "spell" | "amulet"

const TERMS: readonly (readonly [string, Kind])[] = [
  ["リーダー", "leader"],
  ["フォロワー", "follower"],
  ["スペル", "spell"],
  ["アミュレット", "amulet"],
]

/**
 * The kind a rules term names: the authority reports a card's current type as the term of the card
 * text, such as フォロワー・エボルヴ for a follower. Null for a term the screen has no word for.
 */
export function kindOfTerm(term: string): Kind | null {
  return TERMS.find(([part]) => term.includes(part))?.[1] ?? null
}

/** The kind the snapshot prints for a card. */
export function kindOfCode(code: string | null | undefined): Kind | null {
  return code === "leader" || code === "follower" || code === "spell" || code === "amulet"
    ? code
    : null
}

/**
 * A card's cost line. In the zones cards are played from the authority quotes what playing the
 * card costs now, or gives no settled quote, and then the cost is unknown; elsewhere there is only
 * what the card prints.
 */
export type CostLine =
  | { readonly kind: "quoted"; readonly now: number; readonly printed: number | null }
  | { readonly kind: "unknown"; readonly printed: number | null }
  | { readonly kind: "printed"; readonly printed: number }

export function costLine(state: Inspection | null, printed: number | null): CostLine | null {
  if (state !== null && (state.zone === "hand" || state.zone === "ex"))
    return state.card.cost === null
      ? { kind: "unknown", printed }
      : { kind: "quoted", now: state.card.cost, printed }
  return printed === null ? null : { kind: "printed", printed }
}

/**
 * A card's attack and defense. `current` are the numbers the authority reports, with the printed
 * ones beside them when the card prints any; `printed` are the card's own numbers, shown where the
 * authority reports none (a card that is no follower now, or one outside the zones it reports).
 */
export interface StatsLine {
  readonly kind: "current" | "printed"
  readonly attack: number
  readonly defense: number
  readonly printed: { readonly attack: number; readonly defense: number } | null
}

export function statsLine(
  state: Inspection | null,
  attack: number | null,
  defense: number | null,
): StatsLine | null {
  const printed = attack !== null && defense !== null ? { attack, defense } : null
  const stats = state?.card.stats ?? null
  if (stats !== null) return { kind: "current", attack: stats.attack, defense: stats.life, printed }
  return printed === null ? null : { kind: "printed", ...printed, printed }
}
