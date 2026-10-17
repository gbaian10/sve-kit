import { RARITY_COLUMN_MIN_WIDTH } from "../domain/rarityBadges"

export const LIST_COLUMNS = [
  "number",
  "rarity",
  "name",
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
  name: 100,
  cost: 40,
  attack: 40,
  defense: 40,
  effect: 200,
}

export const DEFAULT_LIST_VISIBILITY: ListVisibility = {
  number: true,
  rarity: true,
  name: true,
  cost: true,
  attack: true,
  defense: true,
  effect: true,
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

export function isListVisibility(value: unknown): value is ListVisibility {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false
  const record = value as Record<string, unknown>
  return (
    LIST_COLUMNS.every((column) => typeof record[column] === "boolean") &&
    (record["number"] === true || record["name"] === true)
  )
}
