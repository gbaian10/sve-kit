import { RARITY_COLUMN_MIN_WIDTH } from "../domain/rarityBadges"

export const LIST_COLUMNS = [
  "number",
  "name",
  "rarity",
  "location",
  "type",
  "class",
  "cost",
  "attack",
  "defense",
  "effect",
] as const
export type ListColumn = (typeof LIST_COLUMNS)[number]
export type ListWidths = Readonly<Partial<Record<ListColumn, number>>>
export type ListVisibility = Readonly<Record<ListColumn, boolean>>

export const LIST_MIN_WIDTHS: Readonly<Record<ListColumn, number>> = {
  number: 84,
  rarity: RARITY_COLUMN_MIN_WIDTH,
  location: 72,
  type: 104,
  name: 100,
  class: 48,
  cost: 40,
  attack: 40,
  defense: 40,
  effect: 200,
}

export const DEFAULT_LIST_VISIBILITY: ListVisibility = {
  number: true,
  rarity: true,
  location: true,
  type: true,
  name: true,
  class: true,
  cost: true,
  attack: true,
  defense: true,
  effect: true,
}

export function sanitizeListOrder(value: unknown): readonly ListColumn[] {
  const known = Array.isArray(value)
    ? value.filter((column): column is ListColumn => LIST_COLUMNS.includes(column as ListColumn))
    : []
  const order = [...new Set(known)]
  LIST_COLUMNS.forEach((column, index) => {
    if (order.includes(column)) return
    const previous = LIST_COLUMNS[index - 1]
    order.splice(previous === undefined ? 0 : order.indexOf(previous) + 1, 0, column)
  })
  return order
}

export function moveListColumn(
  order: readonly ListColumn[],
  source: ListColumn,
  target: ListColumn,
  after = false,
): readonly ListColumn[] {
  if (source === target) return order
  const remaining = order.filter((column) => column !== source)
  const index = remaining.indexOf(target)
  if (index < 0) return order
  return [
    ...remaining.slice(0, index + Number(after)),
    source,
    ...remaining.slice(index + Number(after)),
  ]
}

export function isListWidths(value: unknown): value is ListWidths {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false
  const record = value as Record<string, unknown>
  return Object.entries(record).every(([key, width]) =>
    LIST_COLUMNS.some(
      (column) =>
        column === key &&
        typeof width === "number" &&
        Number.isFinite(width) &&
        width >= LIST_MIN_WIDTHS[column],
    ),
  )
}

export function sanitizeListVisibility(value: unknown): ListVisibility {
  const record =
    typeof value === "object" && value !== null && !Array.isArray(value)
      ? (value as Record<string, unknown>)
      : {}
  const visibility = { ...DEFAULT_LIST_VISIBILITY }
  for (const column of LIST_COLUMNS) {
    if (typeof record[column] === "boolean") visibility[column] = record[column]
  }
  return visibility.number || visibility.name ? visibility : DEFAULT_LIST_VISIBILITY
}
