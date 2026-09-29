import { describe, expect, it } from "vitest"

import { formatQuery, parseQuery, querySearch } from "./codec"
import { activeFilterCount, DEFAULT_QUERY, toggleClass } from "./model"

describe("query codec", () => {
  it("reads every parameter and ignores unknown values", () => {
    const state = parseQuery(
      new URLSearchParams(
        "q=%20試作%20&class=witch,elf,elf,Bad!&cost=1-3&type=spell&mech=-kw:a,kw:b&set=set:bp01&rarity=gold&alt=1&unit=art&sort=cost&view=table",
      ),
    )
    expect(state).toEqual({
      text: "試作",
      classes: ["elf", "witch"],
      cost: { min: 1, max: 3 },
      types: ["spell"],
      mechanics: { "kw:a": "not", "kw:b": "has" },
      sets: ["set:bp01"],
      rarities: ["gold"],
      altArtOnly: true,
      unit: "art",
      sort: "cost",
      view: "table",
    })
    expect(parseQuery(new URLSearchParams("unit=nope&sort=nope&view=nope&alt=0"))).toEqual(
      DEFAULT_QUERY,
    )
  })

  it("reads open and single cost bounds and drops impossible ones", () => {
    expect(parseQuery(new URLSearchParams("cost=3")).cost).toEqual({ min: 3, max: 3 })
    expect(parseQuery(new URLSearchParams("cost=-2")).cost).toEqual({ max: 2 })
    expect(parseQuery(new URLSearchParams("cost=5-")).cost).toEqual({ min: 5 })
    expect(parseQuery(new URLSearchParams("cost=5-2")).cost).toEqual({})
    expect(parseQuery(new URLSearchParams("cost=9")).cost).toEqual({})
  })

  it("never writes defaults and writes lists in a stable order", () => {
    expect(formatQuery(DEFAULT_QUERY).toString()).toBe("")
    expect(querySearch(DEFAULT_QUERY)).toBe("")
    const search = querySearch({
      ...DEFAULT_QUERY,
      text: "bp01",
      classes: ["witch", "elf"],
      cost: { min: 1, max: 3 },
      mechanics: { "kw:b": "has", "kw:a": "not" },
    })
    expect(search).toBe("?q=bp01&class=elf%2Cwitch&cost=1-3&mech=-kw%3Aa%2Ckw%3Ab")
    expect(querySearch({ ...DEFAULT_QUERY, cost: { min: 2, max: 2 } })).toBe("?cost=2")
    expect(querySearch({ ...DEFAULT_QUERY, cost: { max: 7 } })).toBe("?cost=-7")
  })

  it("round-trips through the URL", () => {
    const state = parseQuery(
      new URLSearchParams("q=x&class=royal&cost=2-&view=list&unit=printing&sort=name"),
    )
    expect(parseQuery(formatQuery(state))).toEqual(state)
  })

  it("counts active facets for the filter badge", () => {
    expect(activeFilterCount(DEFAULT_QUERY)).toBe(0)
    expect(
      activeFilterCount({
        ...DEFAULT_QUERY,
        text: "x",
        classes: ["elf"],
        cost: { max: 1 },
        altArtOnly: true,
        unit: "art",
      }),
    ).toBe(4)
  })

  it("toggles a class and keeps the list sorted", () => {
    const one = toggleClass(DEFAULT_QUERY, "witch")
    const two = toggleClass(one, "elf")
    expect(two.classes).toEqual(["elf", "witch"])
    expect(toggleClass(two, "witch").classes).toEqual(["elf"])
  })
})
