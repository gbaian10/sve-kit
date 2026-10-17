import { describe, expect, it } from "vitest"

import {
  datedSets,
  groupSets,
  packCounts,
  type ProductFacts,
  recentSets,
  type SetFacts,
  setKindKey,
} from "./sets"

const set = (id: string, kind = "booster"): SetFacts => ({ id, publicCode: id.toUpperCase(), kind })
const product = (
  familyIds: readonly string[],
  region: string,
  releasedOn: string | null,
): ProductFacts => ({ familyIds, region, releasedOn })

describe("datedSets and recentSets", () => {
  it("orders booster sets by their first known release in the region, newest first", () => {
    const sets = [set("s1"), set("s2"), set("s3"), set("s4"), set("deck", "deck")]
    const products = [
      product(["s1"], "jp", "2026-01-10"),
      product(["s1"], "jp", "2026-09-01"),
      product(["s2"], "jp", "2026-03-01"),
      product(["s3"], "jp", "2026-05-01"),
      product(["s4"], "jp", "2026-02-01"),
      product(["deck"], "jp", "2026-12-01"),
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

  it("orders boosters and collaborations together while excluding other kinds", () => {
    const sets = [
      set("booster-old"),
      set("booster-new"),
      set("collaboration-old", "collaboration"),
      set("collaboration-new", "collaboration"),
      set("collaboration-undated", "collaboration"),
      set("collaboration-en", "collaboration"),
      ...["deck", "promo", "special", "special_pack", "other"].map((kind) => set(kind, kind)),
    ]
    const products = [
      product(["booster-old"], "jp", "2026-03-01"),
      product(["booster-new"], "jp", "2026-06-01"),
      product(["collaboration-old"], "jp", "2026-04-01"),
      product(["collaboration-old"], "jp", "2026-10-01"),
      product(["collaboration-new"], "jp", "2026-07-01"),
      product(["collaboration-undated"], "jp", null),
      product(["collaboration-en"], "en", "2026-12-01"),
      ...["deck", "promo", "special", "special_pack", "other"].map((kind) =>
        product([kind], "jp", "2026-12-01"),
      ),
    ]
    expect(recentSets(datedSets(sets, products, "jp"), 3).map((item) => item.id)).toEqual([
      "collaboration-new",
      "booster-new",
      "collaboration-old",
    ])
  })

  it("chooses the latest set separately for each card edition", () => {
    const sets = [set("jp-latest"), set("en-latest"), set("jp-older"), set("en-older")]
    const products = [
      product(["jp-latest"], "jp", "2026-09-01"),
      product(["jp-older"], "jp", "2026-03-01"),
      product(["en-latest"], "en", "2026-08-01"),
      product(["en-older"], "en", "2026-02-01"),
    ]

    expect(recentSets(datedSets(sets, products, "jp"), 1).map((item) => item.id)).toEqual([
      "jp-latest",
    ])
    expect(recentSets(datedSets(sets, products, "en"), 1).map((item) => item.id)).toEqual([
      "en-latest",
    ])
  })

  it("dates every family of a composite product by the earliest regional product", () => {
    const sets = [set("s1"), set("s2"), set("s3", "collaboration")]
    const products = [
      product(["s1", "s2"], "en", "2026-05-01"),
      product(["s1"], "en", "2026-02-01"),
      product(["s2"], "en", "2026-08-01"),
      product(["s1", "s2"], "jp", "2026-01-01"),
      product(["s3"], "en", "2026-04-01"),
    ]
    expect(recentSets(datedSets(sets, products, "en"), 3)).toEqual([
      { ...sets[1], releasedOn: "2026-05-01" },
      { ...sets[2], releasedOn: "2026-04-01" },
      { ...sets[0], releasedOn: "2026-02-01" },
    ])
  })

  it("leaves out sets without a dated, linked product in that region", () => {
    const sets = [set("s1"), set("s2"), set("s3")]
    const products = [
      product(["s1"], "en", "2026-04-01"),
      product([], "jp", "2026-08-01"),
      product(["s2"], "jp", null),
      product(["s3"], "jp", "2026-02-01"),
    ]
    expect(datedSets(sets, products, "jp")).toEqual([
      { id: "s3", publicCode: "S3", kind: "booster", releasedOn: "2026-02-01" },
    ])
    expect(datedSets(sets, [product([], "jp", "2026-08-01")], "jp")).toEqual([])
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

describe("groupSets", () => {
  it("sections the main kinds in a fixed order and keeps every other kind, unknown ones included", () => {
    const sets: { readonly id: string; readonly publicCode: string; readonly kind?: string }[] = [
      set("deck", "deck"),
      set("promo", "promo"),
      set("mystery", "limited_run"),
      set("booster-2"),
      set("collab", "collaboration"),
      set("booster-1"),
      set("special", "special"),
      { id: "untyped", publicCode: "UNTYPED" },
    ]
    const grouped = groupSets(sets)
    expect([...grouped.main.keys()]).toEqual(["booster", "collaboration", "promo"])
    expect(grouped.main.get("booster")?.map((item) => item.id)).toEqual(["booster-2", "booster-1"])
    expect(grouped.main.get("special_pack")).toBeUndefined()
    expect(grouped.other.map((item) => item.id)).toEqual(["deck", "mystery", "special", "untyped"])
  })
})

it("maps every family kind to a copy key, unknown kinds to other", () => {
  expect(
    ["booster", "collaboration", "special_pack", "promo", "deck", "special"].map(setKindKey),
  ).toEqual(["booster", "collaboration", "special_pack", "promo", "deck", "special"])
  expect(setKindKey("other")).toBe("other")
  expect(setKindKey("limited_run")).toBe("other")
  expect(setKindKey(undefined)).toBe("other")
})
