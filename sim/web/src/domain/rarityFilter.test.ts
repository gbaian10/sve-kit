import { describe, expect, it } from "vitest"

import { formatQuery, parseQuery } from "./query/codec"
import { DEFAULT_QUERY } from "./query/model"
import { toggleRarity } from "./rarityFilter"

const children = ["pr:bronze", "pr:silver", "pr:none"]

describe("rarity tree selection", () => {
  it("selects or clears the whole PR family while preserving other official rarities", () => {
    expect(toggleRarity(["sp"], "pr", children)).toEqual(["pr", "sp"])
    expect(toggleRarity(["pr", "sp"], "pr", children)).toEqual(["sp"])
    expect(toggleRarity(["pr:bronze", "sp"], "pr", children)).toEqual(["pr", "sp"])
  })
  it("expands a parent into children and compresses a complete selection back to pr", () => {
    const partial = toggleRarity(["pr", "sp"], "pr:silver", children)
    expect(partial).toEqual(["pr:bronze", "pr:none", "sp"])
    expect(toggleRarity(partial, "pr:silver", children)).toEqual(["pr", "sp"])
  })
  it("round trips legacy PR and union URLs including unrated", () => {
    for (const rarities of [["pr"], ["pr:bronze", "sp"], ["pr:none", "unrated"]]) {
      expect(parseQuery(formatQuery({ ...DEFAULT_QUERY, rarities })).rarities).toEqual(rarities)
    }
  })
})
