import { compareCodePoints } from "../normalize"
import {
  type CostRange,
  DEFAULT_QUERY,
  type MechanicState,
  QUERY_SORTS,
  QUERY_UNITS,
  type QueryState,
  VIEW_MODES,
} from "./model"

// URL parameters: q class cost type mech set rarity alt unit sort view. Lists are comma separated,
// de-duplicated and sorted so equal states always give equal URLs; unknown values are dropped rather
// than failing, because URLs arrive from anywhere. `cost` is `min-max`, `min-` or `-max` (7 = 7+).
const LIST_PARAMS = { classes: "class", types: "type", sets: "set", rarities: "rarity" } as const
const COST_MAX = 7
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

function costBound(text: string): number | undefined {
  if (!/^\d$/u.test(text)) return undefined
  const value = Number(text)
  return value <= COST_MAX ? value : undefined
}

function parseCost(text: string | null): CostRange {
  if (text === null) return {}
  const [minText = "", maxText = ""] = text.includes("-") ? text.split("-", 2) : [text, text]
  const min = costBound(minText)
  const max = costBound(maxText)
  if (min !== undefined && max !== undefined && min > max) return {}
  return { ...(min === undefined ? {} : { min }), ...(max === undefined ? {} : { max }) }
}

function formatCost(cost: CostRange): string | undefined {
  if (cost.min === undefined && cost.max === undefined) return undefined
  if (cost.min !== undefined && cost.min === cost.max) return String(cost.min)
  return `${cost.min === undefined ? "" : String(cost.min)}-${cost.max === undefined ? "" : String(cost.max)}`
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
  return {
    text: params.get("q")?.trim() ?? "",
    classes: list(params, LIST_PARAMS.classes),
    cost: parseCost(params.get("cost")),
    types: list(params, LIST_PARAMS.types),
    mechanics: parseMechanics(params.get("mech")),
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
  const cost = formatCost(state.cost)
  if (cost !== undefined) params.set("cost", cost)
  const mechanics = Object.entries(state.mechanics)
    .sort(([a], [b]) => compareCodePoints(a, b))
    .map(([id, state]) => (state === "not" ? `-${id}` : id))
  if (mechanics.length > 0) params.set("mech", mechanics.join(","))
  if (state.altArtOnly) params.set("alt", "1")
  if (state.unit !== DEFAULT_QUERY.unit) params.set("unit", state.unit)
  if (state.sort !== DEFAULT_QUERY.sort) params.set("sort", state.sort)
  if (state.view !== undefined) params.set("view", state.view)
  return params
}

/** `?q=…` or an empty string: the search string that identifies a list (architecture §2.2). */
export function querySearch(state: QueryState): string {
  const text = formatQuery(state).toString()
  return text === "" ? "" : `?${text}`
}
