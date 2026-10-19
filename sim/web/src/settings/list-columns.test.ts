import { expect, it } from "vitest"

import { LIST_COLUMNS, moveListColumn } from "./list-columns"

it("moves columns in either direction while retaining hidden columns", () => {
  const moved = moveListColumn(LIST_COLUMNS, "class", "effect")
  expect(moved).toEqual([
    "number",
    "name",
    "rarity",
    "evolve",
    "cost",
    "attack",
    "defense",
    "class",
    "effect",
  ])
  expect(moveListColumn(moved, "class", "evolve", true)).toEqual(LIST_COLUMNS)
})
