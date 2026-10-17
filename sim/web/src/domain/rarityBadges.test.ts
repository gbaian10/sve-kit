import { describe, expect, it } from "vitest"

import { selectRarityBadges } from "./rarityBadges"

const base = { region: "jp" as const, premium: false }

describe("rarity badge selection", () => {
  it.each([
    ["br", "br", "br-en"],
    ["sr", "sr", "sr-en"],
    ["gr", "gr", "gr-en"],
    ["lg", "lg", "lg-en"],
    ["ur", "ur", "ur-en"],
    ["sl", "sl", "sl"],
    ["sp", "sp", "sp"],
    ["ssp", "ssp", "ssp"],
  ])("uses the printed JP and EN letters for official %s", (rarityCode, jp, en) => {
    expect(selectRarityBadges({ ...base, rarityCode })).toEqual([jp])
    expect(selectRarityBadges({ ...base, rarityCode, region: "en" })).toEqual([en])
  })

  it("selects no image for PR when appearance is missing or null", () => {
    expect(selectRarityBadges({ ...base, rarityCode: "pr" })).toEqual([])
    expect(
      selectRarityBadges({ ...base, rarityCode: "pr", region: "en", rarityBadge: null }),
    ).toEqual([])
  })

  it.each([
    ["br", "pr-bronze"],
    ["sr", "pr-silver"],
    ["gr", "pr-gold"],
    ["lg", "pr-red"],
    ["sl", "pr-pastel"],
    ["sp", "pr-rainbow"],
  ] as const)("shows a confirmed %s PR", (style, asset) => {
    expect(
      selectRarityBadges({ ...base, rarityCode: "pr", rarityBadge: { style, text: "PR" } }),
    ).toEqual([asset])
  })

  it.each([
    ["br", "BR", "br"],
    ["sr", "SR", "sr"],
    ["gr", "GR", "gr"],
    ["br", "B", "br-en"],
  ] as const)(
    "uses explicit %s/%s letters on an early PR instead of inferring from region",
    (style, text, expected) => {
      for (const region of ["jp", "en"] as const)
        expect(
          selectRarityBadges({
            ...base,
            region,
            rarityCode: "pr",
            rarityBadge: { style, text },
          }),
        ).toEqual([expected])
    },
  )

  it("displays rainbow PR for an official SP without changing its input", () => {
    const input = {
      ...base,
      rarityCode: "sp",
      rarityBadge: { style: "sp" as const, text: "PR" },
    }
    expect(selectRarityBadges(Object.freeze(input))).toEqual(["pr-rainbow"])
    expect(input.rarityCode).toBe("sp")
  })

  it.each(["br", "lg", "-", null])(
    "lets explicit appearance describe the face independently of official %s",
    (rarityCode) => {
      expect(
        selectRarityBadges({
          ...base,
          rarityCode,
          rarityBadge: { style: "sr", text: "SR" },
        }),
      ).toEqual(["sr"])
    },
  )

  it("selects only the official base badge regardless of Premium metadata", () => {
    expect(selectRarityBadges({ ...base, rarityCode: "sr", premium: true })).toEqual(["sr"])
    expect(selectRarityBadges({ ...base, region: "en", rarityCode: "br", premium: true })).toEqual([
      "br-en",
    ])
    expect(selectRarityBadges({ ...base, rarityCode: "pr", premium: true })).toEqual([])
    expect(selectRarityBadges({ ...base, rarityCode: null, premium: true })).toEqual([])
    expect(selectRarityBadges({ ...base, rarityCode: null, premium: null })).toEqual([])
  })

  it("omits official dashes and explicitly absent face labels, including Premium", () => {
    expect(selectRarityBadges({ ...base, rarityCode: "-", premium: true })).toEqual([])
    expect(
      selectRarityBadges({
        ...base,
        rarityCode: "sl",
        premium: true,
        rarityBadge: { style: "none" },
      }),
    ).toEqual([])
  })

  it("falls back to the official code for unsupported appearances and never invents an unknown badge", () => {
    expect(
      selectRarityBadges({
        ...base,
        rarityCode: "pr",
        rarityBadge: { style: "br", text: "UNKNOWN" },
      }),
    ).toEqual([])
    expect(selectRarityBadges({ ...base, rarityCode: "future" })).toEqual([])
    expect(selectRarityBadges({ ...base, rarityCode: "constructor" })).toEqual([])
  })
})
