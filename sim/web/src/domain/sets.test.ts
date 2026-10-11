import { describe, expect, it } from "vitest"

import { datedSets, packCounts, type ProductFacts, recentSets, type SetFacts } from "./sets"

const set = (id: string, kind = "booster"): SetFacts => ({ id, publicCode: id.toUpperCase(), kind })
const product = (
  familyId: string | null,
  region: string,
  releasedOn: string | null,
): ProductFacts => ({ familyId, region, releasedOn })

describe("datedSets and recentSets", () => {
  it("orders booster sets by their first known release in the region, newest first", () => {
    const sets = [set("s1"), set("s2"), set("s3"), set("s4"), set("deck", "deck")]
    const products = [
      product("s1", "jp", "2026-01-10"),
      product("s1", "jp", "2026-09-01"),
      product("s2", "jp", "2026-03-01"),
      product("s3", "jp", "2026-05-01"),
      product("s4", "jp", "2026-02-01"),
      product("deck", "jp", "2026-12-01"),
    ]
    expect(datedSets(sets, products, "jp")).toContainEqual({
      id: "s1",
      publicCode: "S1",
      kind: "booster",
      releasedOn: "2026-01-10",
    })
    expect(recentSets(datedSets(sets, products, "jp"), 3).map((item) => item.id)).toEqual([
      "s3",
      "s2",
      "s4",
    ])
  })

  it("leaves out sets without a dated, linked product in that region", () => {
    const sets = [set("s1"), set("s2"), set("s3")]
    const products = [
      product("s1", "en", "2026-04-01"),
      product(null, "jp", "2026-08-01"),
      product("s2", "jp", null),
      product("s3", "jp", "2026-02-01"),
    ]
    expect(datedSets(sets, products, "jp")).toEqual([
      { id: "s3", publicCode: "S3", kind: "booster", releasedOn: "2026-02-01" },
    ])
    expect(datedSets(sets, [product(null, "jp", "2026-08-01")], "jp")).toEqual([])
  })
})

describe("packCounts", () => {
  it("counts each card once and keeps low-confidence names as translated", () => {
    expect(
      packCounts([
        { key: "c1", translated: true, lowConfidence: false },
        { key: "c1", translated: true, lowConfidence: false },
        { key: "c2", translated: true, lowConfidence: true },
        { key: "c3", translated: false, lowConfidence: false },
        { key: "c4", translated: false, lowConfidence: true },
      ]),
    ).toEqual({ cards: 4, translated: 2, proofreading: 1 })
  })

  it("reports zero only for a complete empty set", () => {
    expect(packCounts([])).toEqual({ cards: 0, translated: 0, proofreading: 0 })
  })
})
