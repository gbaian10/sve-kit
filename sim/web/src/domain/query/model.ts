// The list state of /cards (docs/sim/web-architecture.md §2.1). The URL is its only source of truth:
// every field has one URL parameter, defaults are never written, and results are a pure function of
// it. Vocabulary codes (classes, types, sets, rarities) come from the snapshot; `neutral` stands for
// cards without a class.
export const QUERY_UNITS = ["card", "art", "printing"] as const
export type QueryUnit = (typeof QUERY_UNITS)[number]

export const QUERY_SORTS = ["no", "cost", "name", "date", "atk", "def"] as const
export type QuerySort = (typeof QUERY_SORTS)[number]

export const VIEW_MODES = ["grid", "table", "list"] as const
export type ViewMode = (typeof VIEW_MODES)[number]

export type MechanicState = "has" | "not"

export const NEUTRAL_CLASS = "neutral"

export interface CostRange {
  readonly min?: number
  /** 7 means 7 or more. */
  readonly max?: number
}

export interface QueryState {
  readonly text: string
  readonly classes: readonly string[]
  readonly cost: CostRange
  readonly types: readonly string[]
  /** Keyword ids not listed are "either". */
  readonly mechanics: Readonly<Record<string, MechanicState>>
  readonly sets: readonly string[]
  readonly rarities: readonly string[]
  readonly altArtOnly: boolean
  readonly unit: QueryUnit
  readonly sort: QuerySort
  /** Absent means "use the preference"; only an explicit choice goes into the URL. */
  readonly view?: ViewMode
}

export const DEFAULT_QUERY: QueryState = {
  text: "",
  classes: [],
  cost: {},
  types: [],
  mechanics: {},
  sets: [],
  rarities: [],
  altArtOnly: false,
  unit: "card",
  sort: "no",
}

/** Number of filter facets set beyond the text, for the filter button's badge. */
export function activeFilterCount(state: QueryState): number {
  return (
    (state.classes.length > 0 ? 1 : 0) +
    (state.cost.min !== undefined || state.cost.max !== undefined ? 1 : 0) +
    (state.types.length > 0 ? 1 : 0) +
    (Object.keys(state.mechanics).length > 0 ? 1 : 0) +
    (state.sets.length > 0 ? 1 : 0) +
    (state.rarities.length > 0 ? 1 : 0) +
    (state.altArtOnly ? 1 : 0) +
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
