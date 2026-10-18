import { describe, expect, it } from "vitest"

import {
  cardParamOf,
  clampDrawerWidth,
  drawerFits,
  effectiveDrawerWidth,
  parseCardParam,
  withoutCardParam,
} from "./card-overlay"

describe("drawer width", () => {
  it("takes about a third of a wide window, within 600 to 720", () => {
    expect(effectiveDrawerWidth(null, 1440)).toBe(600)
    expect(effectiveDrawerWidth(null, 1800)).toBe(612)
    expect(effectiveDrawerWidth(null, 2400)).toBe(720)
  })

  it("applies a saved width only as far as the window leaves room for the results", () => {
    expect(effectiveDrawerWidth(1200, 1800)).toBe(1200)
    expect(effectiveDrawerWidth(1200, 1180)).toBe(780)
    expect(effectiveDrawerWidth(1200, 1000)).toBe(600)
    expect(clampDrawerWidth(10, 1440)).toBe(600)
    expect(clampDrawerWidth(5000, 1440)).toBe(1040)
  })

  it("needs room for the smallest drawer and usable results", () => {
    expect(drawerFits(1000)).toBe(true)
    expect(drawerFits(999)).toBe(false)
  })
})

describe("card parameter", () => {
  it("round-trips official and provisional keys", () => {
    expect(cardParamOf({ cardNo: "BP01-001" })).toBe("BP01-001")
    expect(cardParamOf({ cardNo: "", cardNoState: "provisional", intId: 42 })).toBe(
      "_provisional/42",
    )
    expect(parseCardParam("BP01-001")).toEqual({ namespace: "official", key: "BP01-001" })
    expect(parseCardParam("_provisional/42")).toEqual({ namespace: "provisional", key: "42" })
    expect(parseCardParam(null)).toBeNull()
    expect(parseCardParam("")).toBeNull()
  })

  it("drops only the card from a search string", () => {
    expect(withoutCardParam("?q=bp01&card=BP01-001")).toBe("?q=bp01")
    expect(withoutCardParam("?card=BP01-001")).toBe("")
  })
})
