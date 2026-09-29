import {
  type MechanicState,
  NEUTRAL_CLASS,
  type QueryState,
  type QueryUnit,
} from "../../domain/query/model"

export interface FilterChip {
  readonly key: string
  readonly label: string
  readonly remove: () => QueryState
}

/** The mechanics map without one keyword, for chip removal and the sheet's "either" state. */
export function withoutMechanic(
  mechanics: QueryState["mechanics"],
  id: string,
): QueryState["mechanics"] {
  return Object.fromEntries(Object.entries(mechanics).filter(([key]) => key !== id))
}

export interface ChipLabels {
  readonly classLabel: (code: string) => string
  readonly neutralLabel: string
  readonly typeLabel: (code: string) => string
  readonly rarityLabel: (code: string) => string
  readonly costLabel: (range: string) => string
  readonly mechanicLabel: (id: string, wanted: MechanicState) => string
  readonly altLabel: string
  readonly unitLabel: (unit: QueryUnit) => string
}

const costText = (value: number): string => (value === 7 ? "7+" : String(value))

/** The applied facets as chips; each chip knows the state without itself. */
export function chipsFor(state: QueryState, labels: ChipLabels): FilterChip[] {
  const chips: FilterChip[] = []
  for (const code of state.classes)
    chips.push({
      key: `class:${code}`,
      label: code === NEUTRAL_CLASS ? labels.neutralLabel : labels.classLabel(code),
      remove: () => ({ ...state, classes: state.classes.filter((item) => item !== code) }),
    })
  if (state.cost.min !== undefined || state.cost.max !== undefined) {
    const min = state.cost.min ?? 0
    const max = state.cost.max ?? 7
    const range = min === max ? costText(min) : `${costText(min)}–${costText(max)}`
    chips.push({
      key: "cost",
      label: labels.costLabel(range),
      remove: () => ({ ...state, cost: {} }),
    })
  }
  for (const code of state.types)
    chips.push({
      key: `type:${code}`,
      label: labels.typeLabel(code),
      remove: () => ({ ...state, types: state.types.filter((item) => item !== code) }),
    })
  for (const [id, wanted] of Object.entries(state.mechanics))
    chips.push({
      key: `mech:${id}`,
      label: labels.mechanicLabel(id, wanted),
      remove: () => ({ ...state, mechanics: withoutMechanic(state.mechanics, id) }),
    })
  for (const code of state.sets)
    chips.push({
      key: `set:${code}`,
      label: code.toUpperCase(),
      remove: () => ({ ...state, sets: state.sets.filter((item) => item !== code) }),
    })
  for (const code of state.rarities)
    chips.push({
      key: `rarity:${code}`,
      label: labels.rarityLabel(code),
      remove: () => ({ ...state, rarities: state.rarities.filter((item) => item !== code) }),
    })
  if (state.altArtOnly)
    chips.push({
      key: "alt",
      label: labels.altLabel,
      remove: () => ({ ...state, altArtOnly: false }),
    })
  if (state.unit !== "card")
    chips.push({
      key: "unit",
      label: labels.unitLabel(state.unit),
      remove: () => ({ ...state, unit: "card" }),
    })
  return chips
}
