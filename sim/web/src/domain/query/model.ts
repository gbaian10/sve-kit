// The list state of /cards (docs/sim/web-architecture.md §2.1). The URL is its only source of truth:
// every field has one URL parameter, defaults are never written, and results are a pure function of
// it. Vocabulary codes (classes, types, sets, rarities) come from the snapshot; `neutral` stands for
// cards without a class.
import type { RepresentativeRarity } from "../representative"

/** One cell per printing, per art (same card and front-face art) or per card. */
export const QUERY_UNITS = ["printing", "art", "card"] as const
type QueryUnit = (typeof QUERY_UNITS)[number]

/** Registered reprint printings: shown with the rest, hidden, or the only ones shown. */
export const REPRINT_MODES = ["all", "hide", "only"] as const
type ReprintMode = (typeof REPRINT_MODES)[number]

export const QUERY_SORTS = ["no", "cost", "name", "date", "atk", "def"] as const
type QuerySort = (typeof QUERY_SORTS)[number]

export const VIEW_MODES = ["grid", "table", "list"] as const
type ViewMode = (typeof VIEW_MODES)[number]

export type MechanicState = "has" | "not"

export const NEUTRAL_CLASS = "neutral"

/** Cost, attack and defense buttons run 0–9 and "10+"; the cap stands for that number or more. */
export const COST_CAP = 10
export const STAT_CAP = 10

export interface QueryState {
  readonly text: string
  readonly classes: readonly string[]
  /** Selected cost values (0–7, 7 = 7 or more); empty means any cost. */
  readonly cost: readonly number[]
  /** Selected attack values (0–10, 10 = 10 or more); empty means any. */
  readonly attack: readonly number[]
  /** Selected defense values (0–10, 10 = 10 or more); empty means any. */
  readonly defense: readonly number[]
  /** Printed types (`follower`, `spell`, `amulet`); empty means any type. */
  readonly types: readonly string[]
  /** Deck locations (`main`, `evolve`, `token:spell`, `leader:gold`, …); empty means any. */
  readonly locations: readonly string[]
  /** Trait vocabulary codes; a card with any selected trait passes. */
  readonly traits: readonly string[]
  /** Unlisted keyword ids are "either"; has selections form a union, not selections all require absence. */
  readonly mechanics: Readonly<Record<string, MechanicState>>
  readonly sets: readonly string[]
  readonly rarities: readonly string[]
  readonly altArtOnly: boolean
  readonly errataOnly: boolean
  readonly restrictionsOnly: boolean
  /** Filters on registered reprint printings; a merged-by-card list ignores it. */
  readonly reprints: ReprintMode
  readonly unit: QueryUnit
  /** Which printing a merged cell shows (#781); ignored while the unit is `printing`. */
  readonly representative: RepresentativeRarity
  readonly prFirst: boolean
  readonly order?: "desc"
  readonly sort: QuerySort
  /** Absent means "use the preference"; only an explicit choice goes into the URL. */
  readonly view?: ViewMode
}

export const DEFAULT_QUERY: QueryState = {
  text: "",
  classes: [],
  cost: [],
  attack: [],
  defense: [],
  types: [],
  locations: [],
  traits: [],
  mechanics: {},
  sets: [],
  rarities: [],
  altArtOnly: false,
  errataOnly: false,
  restrictionsOnly: false,
  reprints: "all",
  unit: "printing",
  representative: "low",
  prFirst: false,
  sort: "no",
}

/** Number of filter facets set beyond the text, for the filter button's badge. */
export function activeFilterCount(state: QueryState): number {
  return (
    (state.classes.length > 0 ? 1 : 0) +
    (state.cost.length > 0 ? 1 : 0) +
    (state.attack.length > 0 ? 1 : 0) +
    (state.defense.length > 0 ? 1 : 0) +
    (state.types.length > 0 ? 1 : 0) +
    (state.locations.length > 0 ? 1 : 0) +
    (state.traits.length > 0 ? 1 : 0) +
    (Object.keys(state.mechanics).length > 0 ? 1 : 0) +
    (state.sets.length > 0 ? 1 : 0) +
    (state.rarities.length > 0 ? 1 : 0) +
    (state.altArtOnly ? 1 : 0) +
    (state.errataOnly ? 1 : 0) +
    (state.restrictionsOnly ? 1 : 0) +
    (state.reprints !== DEFAULT_QUERY.reprints ? 1 : 0) +
    (state.unit !== DEFAULT_QUERY.unit ? 1 : 0)
  )
}

/** Adds or removes one class code; the result keeps the codes sorted so URLs stay canonical. */
export function toggleClass(state: QueryState, code: string): QueryState {
  const classes = state.classes.includes(code)
    ? state.classes.filter((item) => item !== code)
    : [...state.classes, code].sort()
  return { ...state, classes }
}

/** Adds or removes one numeric value; the result stays sorted so URLs stay canonical. */
export function toggleValue(values: readonly number[], value: number): number[] {
  return values.includes(value)
    ? values.filter((item) => item !== value)
    : [...values, value].sort((a, b) => a - b)
}

/**
 * Whether a card's stat passes a value selection: no selection passes everything, otherwise the
 * stat must be one of the selected values, with the cap collecting every value at or above it.
 */
export function statSelected(
  values: readonly number[],
  cap: number,
  stat: number | null | undefined,
): boolean {
  if (values.length === 0) return true
  if (stat === null || stat === undefined || Number.isNaN(stat)) return false
  return values.includes(Math.min(stat, cap))
}
