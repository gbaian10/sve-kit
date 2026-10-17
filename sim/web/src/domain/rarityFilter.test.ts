import { describe, expect, it } from "vitest"

import { formatQuery, parseQuery } from "./query/codec"
import { DEFAULT_QUERY } from "./query/model"
import { toggleFacet } from "./rarityFilter"

const children = ["pr:br", "pr:sr", "pr:none"]

describe("rarity tree selection", () => {
  it("selects or clears the whole PR family while preserving other official rarities", () => {
    expect(toggleFacet(["sp"], "pr", children, "pr")).toEqual(["pr", "sp"])
    expect(toggleFacet(["pr", "sp"], "pr", children, "pr")).toEqual(["sp"])
    expect(toggleFacet(["pr:br", "sp"], "pr", children, "pr")).toEqual(["pr", "sp"])
  })
  it("expands a parent into children and compresses a complete selection back to pr", () => {
    const partial = toggleFacet(["pr", "sp"], "pr:sr", children, "pr")
    expect(partial).toEqual(["pr:br", "pr:none", "sp"])
    expect(toggleFacet(partial, "pr:sr", children, "pr")).toEqual(["pr", "sp"])
  })
  it("round trips PR and union URLs including unrated", () => {
    for (const rarities of [["pr"], ["pr:br", "sp"], ["pr:none", "unrated"]]) {
      expect(parseQuery(formatQuery({ ...DEFAULT_QUERY, rarities })).rarities).toEqual(rarities)
    }
  })
})
