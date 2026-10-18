import { describe, expect, it } from "vitest"

import { pickRepresentative, RARITY_LADDER, rarityRank } from "./representative"

interface Printing {
  readonly id: string
  readonly rarity: string
  readonly order: number
}
const printing = (id: string, rarity: string, order: number): Printing => ({ id, rarity, order })
const pick = (
  candidates: readonly Printing[],
  rarity: "low" | "high",
  prFirst = false,
): string | undefined =>
  pickRepresentative(
    candidates,
    (candidate) => candidate.rarity,
    (candidate) => candidate.order,
    { rarity, prFirst },
  )?.id

// The maintainer's example: LG and SL share one art, UR is another; a PR printing is added here.
const LG = printing("BP01-001", "lg", 1)
const SL = printing("BP01-SL01", "sl", 2)
const UR = printing("BP01-U01", "ur", 3)
const PR = printing("PR-010", "pr", 9)

describe("rarity ladder", () => {
  it("ranks BR below SSP and everything off the ladder below BR", () => {
    expect([...RARITY_LADDER].map(rarityRank)).toEqual([0, 1, 2, 3, 4, 5, 6, 7])
    expect(rarityRank("pr")).toBe(-1)
    expect(rarityRank("")).toBe(-1)
    expect(rarityRank("-")).toBe(-1)
    expect(rarityRank(null)).toBe(-1)
    expect(rarityRank(undefined)).toBe(-1)
  })
})

describe("representative printing", () => {
  it("takes the lowest or highest ladder rank among the non-PR printings", () => {
    expect(pick([LG, SL, UR], "low")).toBe("BP01-001")
    expect(pick([LG, SL, UR], "high")).toBe("BP01-U01")
    expect(pick([LG, SL], "high")).toBe("BP01-SL01")
    expect(pick([UR], "low")).toBe("BP01-U01")
  })

  it("prefers a PR only when asked, and ignores PR otherwise", () => {
    expect(pick([LG, SL, UR, PR], "low")).toBe("BP01-001")
    expect(pick([LG, SL, UR, PR], "high")).toBe("BP01-U01")
    expect(pick([LG, SL, UR, PR], "low", true)).toBe("PR-010")
    expect(pick([LG, SL, UR, PR], "high", true)).toBe("PR-010")
    expect(pick([LG, SL, UR], "low", true)).toBe("BP01-001")
  })

  it("shows the PR when the group has nothing else, the earliest number among several", () => {
    expect(pick([PR], "low")).toBe("PR-010")
    expect(pick([PR], "high")).toBe("PR-010")
    expect(pick([printing("PR-020", "pr", 12), PR], "high")).toBe("PR-010")
    expect(pick([printing("PR-020", "pr", 12), PR], "low", true)).toBe("PR-010")
  })

  it("breaks a rarity tie by the earliest card number", () => {
    const reprint = printing("SD01-005", "lg", 20)
    expect(pick([reprint, LG], "low")).toBe("BP01-001")
    expect(pick([reprint, LG], "high")).toBe("BP01-001")
    expect(pick([reprint, printing("BP01-001", "lg", 30)], "low")).toBe("SD01-005")
  })

  it("treats printings without a rarity as the lowest tier", () => {
    const token = printing("BP01-T01", "", 50)
    const leader = printing("BP01-L01", "-", 60)
    expect(pick([token, LG], "low")).toBe("BP01-T01")
    expect(pick([token, LG], "high")).toBe("BP01-001")
    expect(pick([token, leader], "low")).toBe("BP01-T01")
    expect(pick([leader, token], "high")).toBe("BP01-T01")
    expect(pick([token], "high")).toBe("BP01-T01")
  })

  it("returns nothing for an empty group", () => {
    expect(pick([], "low")).toBeUndefined()
    expect(pick([], "high", true)).toBeUndefined()
  })
})
