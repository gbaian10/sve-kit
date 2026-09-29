import { describe, expect, it } from "vitest"

import { formatQuery, parseQuery, querySearch } from "./codec"
import { activeFilterCount, DEFAULT_QUERY } from "./model"

describe("query codec", () => {
  it("reads every parameter and ignores unknown values", () => {
    const state = parseQuery(
      new URLSearchParams(
        "q=%20試作%20&class=witch,elf,elf&cost=3,1,9,x&type=spell&mech=-kw:a,kw:b&set=set:bp01&rarity=gold&alt=1&unit=art&sort=cost&view=table",
      ),
    )
    expect(state).toEqual({
      text: "試作",
      classes: ["elf", "witch"],
      cost: [1, 3],
      types: ["spell"],
      mechanics: [
        { id: "kw:a", state: "not" },
        { id: "kw:b", state: "has" },
      ],
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

  it("never writes defaults and writes lists in a stable order", () => {
    expect(formatQuery(DEFAULT_QUERY).toString()).toBe("")
    expect(querySearch(DEFAULT_QUERY)).toBe("")
    const search = querySearch({
      ...DEFAULT_QUERY,
      text: "bp01",
      classes: ["witch", "elf"],
      cost: [3, 1],
      mechanics: [
        { id: "kw:b", state: "has" },
        { id: "kw:a", state: "not" },
      ],
    })
    expect(search).toBe("?q=bp01&class=elf%2Cwitch&cost=1%2C3&mech=-kw%3Aa%2Ckw%3Ab")
  })

  it("round-trips through the URL", () => {
    const state = parseQuery(new URLSearchParams("q=x&class=royal&view=list&unit=printing"))
    expect(parseQuery(formatQuery(state))).toEqual(state)
  })

  it("counts active facets for the filter badge", () => {
    expect(activeFilterCount(DEFAULT_QUERY)).toBe(0)
    expect(
      activeFilterCount({
        ...DEFAULT_QUERY,
        text: "x",
        classes: ["elf"],
        cost: [1],
        altArtOnly: true,
        unit: "art",
      }),
    ).toBe(4)
  })
})
