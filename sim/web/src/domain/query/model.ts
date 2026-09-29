// The list state of /cards. The URL is its only source of truth (architecture §2.2.1): every field
// here has one URL parameter, defaults are never written, and results are a pure function of it.
export const QUERY_UNITS = ["card", "art", "printing"] as const
export type QueryUnit = (typeof QUERY_UNITS)[number]

export const QUERY_SORTS = ["set", "cost", "name"] as const
export type QuerySort = (typeof QUERY_SORTS)[number]

export const VIEW_MODES = ["grid", "table", "list"] as const
export type ViewMode = (typeof VIEW_MODES)[number]

export type MechanicState = "has" | "not"

export interface MechanicFilter {
  readonly id: string
  readonly state: MechanicState
}

export interface QueryState {
  readonly text: string
  readonly classes: readonly string[]
  readonly cost: readonly number[]
  readonly types: readonly string[]
  readonly mechanics: readonly MechanicFilter[]
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
  cost: [],
  types: [],
  mechanics: [],
  sets: [],
  rarities: [],
  altArtOnly: false,
  unit: "card",
  sort: "set",
}

/** Number of filter facets set beyond the text, for the filter button's badge. */
export function activeFilterCount(state: QueryState): number {
  return (
    (state.classes.length > 0 ? 1 : 0) +
    (state.cost.length > 0 ? 1 : 0) +
    (state.types.length > 0 ? 1 : 0) +
    (state.mechanics.length > 0 ? 1 : 0) +
    (state.sets.length > 0 ? 1 : 0) +
    (state.rarities.length > 0 ? 1 : 0) +
    (state.altArtOnly ? 1 : 0) +
    (state.unit !== DEFAULT_QUERY.unit ? 1 : 0)
  )
}
