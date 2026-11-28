import { describe, expect, it } from "vitest"

import { adoptPrPackRarity } from "./prPack"

describe("PR pack rarity follows the printed badge", () => {
  // PR-108 (Token) is listed as PR, PR-017 (leader) as "-"; neither has a badge.
  const token = {
    rarity_code: "pr",
    rarity_raw: "PR",
    premium: false,
    rarity_badge: { style: "none" },
  }
  const leader = { rarity_code: null, rarity_raw: "-", premium: null, rarity_badge: null }

  it("shows no rarity for a printing without a badge, whatever the site lists", () => {
    for (const row of [token, leader, { ...token, rarity_badge: null }])
      expect(adoptPrPackRarity(row, "pr")).toEqual({
        rarity_code: null,
        rarity_raw: "-",
        premium: null,
        rarity_badge: null,
      })
  })

  it("rates a badged printing as PR even when the site lists something else", () => {
    const badge = { style: "sr", text: "PR" }
    expect(adoptPrPackRarity({ ...leader, rarity_badge: badge }, "pr")).toEqual({
      rarity_code: "pr",
      rarity_raw: "-",
      premium: false,
      rarity_badge: badge,
    })
  })

  it("leaves other families and unclassified locator rows alone", () => {
    expect(adoptPrPackRarity(token, "bp01")).toBe(token)
    const locator = { id: "p:pr-108", card_no: "PR-108" }
    expect(adoptPrPackRarity(locator, "pr")).toBe(locator)
  })
})
