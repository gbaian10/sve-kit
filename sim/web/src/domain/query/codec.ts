import { compareCodePoints } from "../normalize"
import {
  DEFAULT_QUERY,
  type MechanicFilter,
  QUERY_SORTS,
  QUERY_UNITS,
  type QueryState,
  VIEW_MODES,
} from "./model"

// URL parameters: q class cost type mech set rarity alt unit sort view. Lists are comma separated,
// de-duplicated and sorted so equal states always give equal URLs; unknown values are dropped
// rather than failing, because URLs arrive from anywhere.
const LIST_PARAMS = { classes: "class", types: "type", sets: "set", rarities: "rarity" } as const
const COST_MAX = 7

function list(params: URLSearchParams, name: string): string[] {
  const values =
    params
      .get(name)
      ?.split(",")
      .map((value) => value.trim())
      .filter((value) => value !== "") ?? []
  return [...new Set(values)].sort(compareCodePoints)
}

function oneOf<T extends string>(value: string | null, allowed: readonly T[]): T | undefined {
  return allowed.find((item) => item === value)
}

export function parseQuery(params: URLSearchParams): QueryState {
  const cost = [
    ...new Set(
      list(params, "cost")
        .map(Number)
        .filter((n) => Number.isInteger(n) && n >= 0 && n <= COST_MAX),
    ),
  ].sort((a, b) => a - b)
  const mechanics: MechanicFilter[] = list(params, "mech").flatMap((entry) => {
    const [state, id] = entry.startsWith("-")
      ? (["not", entry.slice(1)] as const)
      : (["has", entry] as const)
    return id === "" ? [] : [{ id, state }]
  })
  const unique = new Map(mechanics.map((item) => [item.id, item]))
  const view = oneOf(params.get("view"), VIEW_MODES)
  return {
    text: params.get("q")?.trim() ?? "",
    classes: list(params, LIST_PARAMS.classes),
    cost,
    types: list(params, LIST_PARAMS.types),
    mechanics: [...unique.values()].sort((a, b) => compareCodePoints(a.id, b.id)),
    sets: list(params, LIST_PARAMS.sets),
    rarities: list(params, LIST_PARAMS.rarities),
    altArtOnly: params.get("alt") === "1",
    unit: oneOf(params.get("unit"), QUERY_UNITS) ?? DEFAULT_QUERY.unit,
    sort: oneOf(params.get("sort"), QUERY_SORTS) ?? DEFAULT_QUERY.sort,
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
  if (state.cost.length > 0)
    params.set("cost", [...new Set(state.cost)].sort((a, b) => a - b).join(","))
  if (state.mechanics.length > 0) {
    const entries = [...state.mechanics]
      .sort((a, b) => compareCodePoints(a.id, b.id))
      .map((item) => (item.state === "not" ? `-${item.id}` : item.id))
    params.set("mech", entries.join(","))
  }
  if (state.altArtOnly) params.set("alt", "1")
  if (state.unit !== DEFAULT_QUERY.unit) params.set("unit", state.unit)
  if (state.sort !== DEFAULT_QUERY.sort) params.set("sort", state.sort)
  if (state.view !== undefined) params.set("view", state.view)
  return params
}

/** `?q=…` or an empty string, for building hrefs. */
export function querySearch(state: QueryState): string {
  const text = formatQuery(state).toString()
  return text === "" ? "" : `?${text}`
}
