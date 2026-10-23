import { describe, expect, it } from "vitest"

import { formatQuery, parseQuery, querySearch } from "./codec"
import { activeFilterCount, DEFAULT_QUERY, statSelected, toggleClass, toggleValue } from "./model"

describe("query codec", () => {
  it("reads every parameter and ignores unknown values", () => {
    const state = parseQuery(
      new URLSearchParams(
        "q=%20試作%20&class=witch,elf,elf,Bad!&cost=3,1,3&atk=10,0&def=4&type=spell&loc=token:spell,main&trait=pixie,Bad!&mech=-kw:a,kw:b&set=set:bp01&rarity=gold&alt=1&unit=art&rep=high&prfirst=1&sort=cost&view=table",
      ),
    )
    expect(state).toEqual({
      text: "試作",
      classes: ["elf", "witch"],
      cost: [1, 3],
      attack: [0, 10],
      defense: [4],
      types: ["spell"],
      locations: ["main", "token:spell"],
      traits: ["pixie"],
      mechanics: { "kw:a": "not", "kw:b": "has" },
      sets: ["set:bp01"],
      rarities: ["gold"],
      altArtOnly: true,
      errataOnly: false,
      unit: "art",
      representative: "high",
      prFirst: true,
      sort: "cost",
      view: "table",
    })
    expect(
      parseQuery(new URLSearchParams("unit=nope&rep=nope&prfirst=0&sort=nope&view=nope&alt=0")),
    ).toEqual(DEFAULT_QUERY)
  })

  it("keeps the merged-cell settings in the URL and still reads the older card-unit links", () => {
    expect(parseQuery(new URLSearchParams("unit=card"))).toMatchObject({
      unit: "card",
      representative: "low",
      prFirst: false,
    })
    const merged = { ...DEFAULT_QUERY, unit: "art" as const, representative: "high" as const }
    expect(querySearch(merged)).toBe("?unit=art&rep=high")
    expect(querySearch({ ...merged, prFirst: true })).toBe("?unit=art&rep=high&prfirst=1")
    expect(parseQuery(formatQuery({ ...merged, prFirst: true }))).toEqual({
      ...merged,
      prFirst: true,
    })
    // The preference stays in the URL while every printing is shown, so switching back keeps it.
    expect(querySearch({ ...DEFAULT_QUERY, representative: "high", prFirst: true })).toBe(
      "?rep=high&prfirst=1",
    )
  })

  it("reads deck locations out of older type URLs and writes them back as loc", () => {
    const state = parseQuery(
      new URLSearchParams("type=token,leader:gold,ep,spell,follower&loc=main,leader:gold"),
    )
    expect(state.types).toEqual(["follower", "spell"])
    expect(state.locations).toEqual(["ep", "leader:gold", "main", "token"])
    expect(formatQuery(state).toString()).toBe(
      "type=follower%2Cspell&loc=ep%2Cleader%3Agold%2Cmain%2Ctoken",
    )
    expect(parseQuery(formatQuery(state))).toEqual(state)
    expect(querySearch({ ...DEFAULT_QUERY, traits: ["machina", "pixie"] })).toBe(
      "?trait=machina%2Cpixie",
    )
  })

  it("defaults to printing rows in card-number order and preserves explicit card URLs", () => {
    expect(parseQuery(new URLSearchParams())).toMatchObject({ unit: "printing", sort: "no" })
    expect(formatQuery(parseQuery(new URLSearchParams("unit=printing"))).toString()).toBe("")
    expect(formatQuery(parseQuery(new URLSearchParams("unit=card"))).toString()).toBe("unit=card")
  })

  it("reads stat values up to their cap and drops anything else", () => {
    expect(parseQuery(new URLSearchParams("cost=7,2")).cost).toEqual([2, 7])
    expect(parseQuery(new URLSearchParams("cost=8,11")).cost).toEqual([8])
    expect(parseQuery(new URLSearchParams("cost=2-4&atk=-1,x,1.5,10&def=11")).cost).toEqual([])
    expect(parseQuery(new URLSearchParams("atk=-1,x,1.5,10")).attack).toEqual([10])
    expect(parseQuery(new URLSearchParams("def=11")).defense).toEqual([])
  })

  it("never writes defaults and writes lists in a stable order", () => {
    expect(formatQuery(DEFAULT_QUERY).toString()).toBe("")
    expect(querySearch(DEFAULT_QUERY)).toBe("")
    const search = querySearch({
      ...DEFAULT_QUERY,
      text: "bp01",
      classes: ["witch", "elf"],
      cost: [3, 1, 3],
      defense: [10, 0],
      mechanics: { "kw:b": "has", "kw:a": "not" },
    })
    expect(search).toBe("?q=bp01&class=elf%2Cwitch&cost=1%2C3&def=0%2C10&mech=-kw%3Aa%2Ckw%3Ab")
    expect(querySearch({ ...DEFAULT_QUERY, cost: [2] })).toBe("?cost=2")
    expect(querySearch({ ...DEFAULT_QUERY, attack: [12, -1] })).toBe("")
  })

  it("drops non-integer and out-of-range stat values before writing and round-trips the rest", () => {
    const state = {
      ...DEFAULT_QUERY,
      cost: [1.5, 7, Number.NaN, 2, 11, -0.5],
      defense: [10.000001],
    }
    const params = formatQuery(state)
    expect(params.get("cost")).toBe("2,7")
    expect(params.has("def")).toBe(false)
    expect(parseQuery(params)).toEqual({ ...DEFAULT_QUERY, cost: [2, 7] })
  })

  it("round-trips through the URL", () => {
    const state = parseQuery(
      new URLSearchParams(
        "q=x&class=royal&cost=2,7&atk=0&def=10&view=list&unit=printing&sort=name&order=desc",
      ),
    )
    expect(state.order).toBe("desc")
    expect(formatQuery(state).get("order")).toBe("desc")
    expect(parseQuery(formatQuery(state))).toEqual(state)
  })

  it("counts active facets for the filter badge", () => {
    expect(activeFilterCount(DEFAULT_QUERY)).toBe(0)
    expect(
      activeFilterCount({
        ...DEFAULT_QUERY,
        text: "x",
        classes: ["elf", "bishop"],
        cost: [1],
        attack: [2],
        altArtOnly: true,
        unit: "art",
      }),
    ).toBe(5)
    expect(
      activeFilterCount({
        ...DEFAULT_QUERY,
        locations: ["main"],
        traits: ["pixie"],
        types: ["spell"],
      }),
    ).toBe(3)
  })

  it("toggles a stat value and keeps the list in numeric order", () => {
    expect(toggleValue([], 7)).toEqual([7])
    expect(toggleValue([7], 2)).toEqual([2, 7])
    expect(toggleValue([2, 7], 7)).toEqual([2])
  })

  it("matches a stat against the selection, collecting values at or above the cap", () => {
    expect(statSelected([], 7, null)).toBe(true)
    expect(statSelected([2, 7], 7, 2)).toBe(true)
    expect(statSelected([2, 7], 7, 3)).toBe(false)
    expect(statSelected([7], 7, 12)).toBe(true)
    expect(statSelected([0], 7, null)).toBe(false)
    expect(statSelected([0], 7, Number.NaN)).toBe(false)
  })

  it("toggles a class and keeps the list sorted", () => {
    const one = toggleClass(DEFAULT_QUERY, "witch")
    const two = toggleClass(one, "elf")
    expect(two.classes).toEqual(["elf", "witch"])
    expect(toggleClass(two, "witch").classes).toEqual(["elf"])
  })
})
