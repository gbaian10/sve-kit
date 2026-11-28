import { describe, expect, it } from "vitest"

import { formatQuery, parseQuery } from "./query/codec"
import { DEFAULT_QUERY } from "./query/model"
import {
  filterRarity,
  matchesRaritySelection,
  orderRarityTokens,
  PREMIUM_RARITY,
  toggleFacet,
} from "./rarityFilter"

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

describe("premium rarity selection", () => {
  it("matches the BP01 official rarity combinations", () => {
    // The checked totals are 56 ordinary SR, 18 SR Premium and 35 Premium overall.
    const bp01 = [
      ...Array.from({ length: 56 }, () => ({ rarity: "sr", premium: false })),
      ...Array.from({ length: 18 }, () => ({ rarity: "sr", premium: true })),
      ...Array.from({ length: 17 }, () => ({ rarity: "other", premium: true })),
      ...Array.from({ length: 182 }, () => ({ rarity: "other", premium: false })),
    ]
    const count = (selected: readonly string[]) =>
      bp01.filter((printing) =>
        matchesRaritySelection(
          { rarityCode: printing.rarity, premium: printing.premium },
          selected,
        ),
      ).length

    expect(count(["sr"])).toBe(56)
    expect(count([PREMIUM_RARITY])).toBe(35)
    expect(count(["sr", PREMIUM_RARITY])).toBe(18)
  })

  it("keeps unrated premium printings unrated and intersects premium with PR", () => {
    const matches = (rarityCode: string | null, premium: boolean, selected: string[]) =>
      matchesRaritySelection({ rarityCode, premium }, selected)
    expect(matches(null, true, ["unrated", PREMIUM_RARITY])).toBe(true)
    expect(matches("-", true, ["unrated", PREMIUM_RARITY])).toBe(true)
    expect(matches("pr", true, ["pr", PREMIUM_RARITY])).toBe(true)
    expect(matches("pr", false, ["pr", PREMIUM_RARITY])).toBe(false)
    expect(matches("br", true, ["pr", PREMIUM_RARITY])).toBe(false)
  })

  it("orders the premium token after printed rarities and before PR", () => {
    expect(orderRarityTokens(["pr", PREMIUM_RARITY, "ssp", "sr"])).toEqual([
      "sr",
      "ssp",
      PREMIUM_RARITY,
      "pr",
    ])
  })

  it("orders GBF between SL and UR, and its PR appearance the same way", () => {
    expect(orderRarityTokens(["pr:none", "pr:sp", "ur", "pr:gbf", "gbf", "pr:sl", "sl"])).toEqual([
      "sl",
      "gbf",
      "ur",
      "pr:sl",
      "pr:gbf",
      "pr:sp",
      "pr:none",
    ])
  })
})

describe("PR pack printings without an official rarity", () => {
  // PR-017 (leader) and PR-266 (EP) are listed with "-"; PR-660 carries an official PR.
  const prLeader = { rarityCode: "-", premium: false, owner: "pr" }
  const prEp = { rarityCode: null, premium: false, owner: "pr" }
  const prWithRarity = { rarityCode: "pr", premium: false, badgeStyle: "none", owner: "pr" }
  const bp01Leader = { rarityCode: "-", premium: false, owner: "bp01" }

  it("files them under PR without a badge, not under unrated", () => {
    for (const printing of [prLeader, prEp]) {
      expect(filterRarity(printing)).toEqual({ rarity: "pr", style: "none" })
      for (const selected of [["pr"], ["pr:none"], ["pr:sr", "pr:none"]])
        expect(matchesRaritySelection(printing, selected)).toBe(true)
      for (const selected of [["unrated"], ["pr:sr"], ["pr:unknown"], ["br"]])
        expect(matchesRaritySelection(printing, selected)).toBe(false)
    }
    expect(matchesRaritySelection(prWithRarity, ["pr:none"])).toBe(true)
  })

  it("keeps unrated printings outside the PR pack unrated", () => {
    for (const printing of [bp01Leader, { rarityCode: null, premium: false }]) {
      expect(filterRarity(printing)).toEqual({ rarity: "unrated" })
      expect(matchesRaritySelection(printing, ["unrated"])).toBe(true)
      expect(matchesRaritySelection(printing, ["pr"])).toBe(false)
    }
  })
})
