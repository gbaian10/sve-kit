import { isLocationToken } from "../deckLocation"
import { compareCodePoints } from "../normalize"
import { REPRESENTATIVE_RARITIES } from "../representative"
import {
  COST_CAP,
  DEFAULT_QUERY,
  type MechanicState,
  QUERY_SORTS,
  QUERY_UNITS,
  type QueryState,
  STAT_CAP,
  VIEW_MODES,
} from "./model"

// URL parameters: q class cost atk def type loc trait mech set rarity alt unit rep prfirst sort
// order view. Lists
// are comma separated, de-duplicated and sorted so equal states always give equal URLs; unknown
// values are dropped rather than failing, because URLs arrive from anywhere. `cost`, `atk` and `def`
// list the selected values; the cap (7 for cost, 10 for attack and defense) means that value or more.
// Deck locations used to live in `type`, so location tokens found there still read as `loc`.
const LIST_PARAMS = {
  classes: "class",
  types: "type",
  locations: "loc",
  traits: "trait",
  sets: "set",
  rarities: "rarity",
} as const
const VALUE_PARAMS = {
  cost: ["cost", COST_CAP],
  attack: ["atk", STAT_CAP],
  defense: ["def", STAT_CAP],
} as const
const CODE = /^[a-z][a-z0-9_:-]*$/u

function list(params: URLSearchParams, name: string): string[] {
  const values =
    params
      .get(name)
      ?.split(",")
      .map((value) => value.trim())
      .filter((value) => CODE.test(value)) ?? []
  return [...new Set(values)].sort(compareCodePoints)
}

function oneOf<T extends string>(value: string | null, allowed: readonly T[]): T | undefined {
  return allowed.find((item) => item === value)
}

function values(params: URLSearchParams, name: string, cap: number): number[] {
  const selected =
    params
      .get(name)
      ?.split(",")
      .map((value) => value.trim())
      .filter((value) => /^\d{1,2}$/u.test(value))
      .map(Number)
      .filter((value) => value <= cap) ?? []
  return [...new Set(selected)].sort((a, b) => a - b)
}

function parseMechanics(text: string | null): Record<string, MechanicState> {
  const out: Record<string, MechanicState> = {}
  for (const entry of text?.split(",") ?? []) {
    const state: MechanicState = entry.startsWith("-") ? "not" : "has"
    const id = state === "not" ? entry.slice(1) : entry
    if (CODE.test(id)) out[id] = state
  }
  return out
}

export function parseQuery(params: URLSearchParams): QueryState {
  const view = oneOf(params.get("view"), VIEW_MODES)
  const typed = list(params, LIST_PARAMS.types)
  return {
    text: params.get("q")?.trim() ?? "",
    classes: list(params, LIST_PARAMS.classes),
    cost: values(params, VALUE_PARAMS.cost[0], VALUE_PARAMS.cost[1]),
    attack: values(params, VALUE_PARAMS.attack[0], VALUE_PARAMS.attack[1]),
    defense: values(params, VALUE_PARAMS.defense[0], VALUE_PARAMS.defense[1]),
    types: typed.filter((token) => !isLocationToken(token)),
    locations: [
      ...new Set([...list(params, LIST_PARAMS.locations), ...typed.filter(isLocationToken)]),
    ].sort(compareCodePoints),
    traits: list(params, LIST_PARAMS.traits),
    mechanics: parseMechanics(params.get("mech")),
    sets: list(params, LIST_PARAMS.sets),
    rarities: list(params, LIST_PARAMS.rarities),
    altArtOnly: params.get("alt") === "1",
    unit: oneOf(params.get("unit"), QUERY_UNITS) ?? DEFAULT_QUERY.unit,
    representative:
      oneOf(params.get("rep"), REPRESENTATIVE_RARITIES) ?? DEFAULT_QUERY.representative,
    prFirst: params.get("prfirst") === "1",
    sort: oneOf(params.get("sort"), QUERY_SORTS) ?? DEFAULT_QUERY.sort,
    ...(params.get("order") === "desc" ? { order: "desc" as const } : {}),
    ...(view === undefined ? {} : { view }),
  }
}

export function formatQuery(state: QueryState): URLSearchParams {
  const params = new URLSearchParams()
  if (state.text !== "") params.set("q", state.text)
  for (const [field, name] of Object.entries(LIST_PARAMS) as [keyof typeof LIST_PARAMS, string][]) {
    const values = [...new Set(state[field])].sort(compareCodePoints)
    if (values.length > 0) params.set(name, values.join(","))
  }
  for (const [field, [name, cap]] of Object.entries(VALUE_PARAMS) as [
    keyof typeof VALUE_PARAMS,
    readonly [string, number],
  ][]) {
    const selected = [
      ...new Set(
        state[field].filter((value) => Number.isInteger(value) && value >= 0 && value <= cap),
      ),
    ].sort((a, b) => a - b)
    if (selected.length > 0) params.set(name, selected.join(","))
  }
  const mechanics = Object.entries(state.mechanics)
    .sort(([a], [b]) => compareCodePoints(a, b))
    .map(([id, state]) => (state === "not" ? `-${id}` : id))
  if (mechanics.length > 0) params.set("mech", mechanics.join(","))
  if (state.altArtOnly) params.set("alt", "1")
  if (state.unit !== DEFAULT_QUERY.unit) params.set("unit", state.unit)
  if (state.representative !== DEFAULT_QUERY.representative) params.set("rep", state.representative)
  if (state.prFirst) params.set("prfirst", "1")
  if (state.sort !== DEFAULT_QUERY.sort) params.set("sort", state.sort)
  if (state.order === "desc") params.set("order", "desc")
  if (state.view !== undefined) params.set("view", state.view)
  return params
}

/** `?q=…` or an empty string: the search string that identifies a list (architecture §2.2). */
export function querySearch(state: QueryState): string {
  const text = formatQuery(state).toString()
  return text === "" ? "" : `?${text}`
}

/** Lists use JP unless their own URL explicitly selects EN. */
export function editionUrl(path: string, edition: "jp" | "en"): string {
  const [pathname = "", search = ""] = path.split("?", 2)
  const params = new URLSearchParams(search)
  if (edition === "en") params.set("ed", "en")
  else params.delete("ed")
  return `${pathname}${params.size ? `?${params.toString()}` : ""}`
}
