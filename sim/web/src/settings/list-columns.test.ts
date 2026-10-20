import { expect, it } from "vitest"

import {
  DEFAULT_LIST_VISIBILITY,
  LIST_COLUMNS,
  moveListColumn,
  sanitizeListOrder,
  sanitizeListVisibility,
} from "./list-columns"

it("fills an eight-column preference after its default predecessors and preserves hidden columns", () => {
  const order = ["number", "name", "rarity", "class", "cost", "attack", "defense", "effect"]
  const visibility = Object.fromEntries(order.map((column) => [column, column !== "rarity"]))
  expect(sanitizeListOrder(order)).toEqual(LIST_COLUMNS)
  expect(sanitizeListVisibility(visibility)).toEqual({ ...DEFAULT_LIST_VISIBILITY, rarity: false })
})

it("defaults invalid individual visibility values while retaining a valid identity column", () => {
  expect(
    sanitizeListVisibility({ number: false, name: true, rarity: false, cost: "false" }),
  ).toEqual({
    ...DEFAULT_LIST_VISIBILITY,
    number: false,
    rarity: false,
  })
  expect(sanitizeListVisibility({ number: false, name: false })).toEqual(DEFAULT_LIST_VISIBILITY)
})

it("moves columns in either direction while retaining hidden columns", () => {
  const moved = moveListColumn(LIST_COLUMNS, "class", "effect")
  expect(moved).toEqual([
    "number",
    "name",
    "rarity",
    "location",
    "type",
    "cost",
    "attack",
    "defense",
    "class",
    "effect",
  ])
  expect(moveListColumn(moved, "class", "type", true)).toEqual(LIST_COLUMNS)
})
