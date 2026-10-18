import { describe, expect, it } from "vitest"

import {
  isLocationToken,
  leaderFrameTokens,
  orderLocationTokens,
  tokenKindTokens,
} from "./deckLocation"
import { orderTypeTokens } from "./typeFilter"

describe("deck location tokens", () => {
  it("tells deck locations and their sub-values from printed types", () => {
    for (const token of [
      "main",
      "evolve",
      "advance",
      "leader",
      "leader:gold",
      "token",
      "token:spell",
      "ep",
      "sep",
    ])
      expect(isLocationToken(token), token).toBe(true)
    for (const token of ["follower", "spell", "amulet", "crest", "", "mainly"])
      expect(isLocationToken(token), token).toBe(false)
  })

  it("offers only the sub-values with printings, in display order", () => {
    expect(tokenKindTokens({ "token:crest": 1, "token:follower": 2, "token:spell": 0 })).toEqual([
      "token:follower",
      "token:crest",
    ])
    expect(leaderFrameTokens({ "leader:unknown": 1, "leader:gold": 3 })).toEqual([
      "leader:gold",
      "leader:unknown",
    ])
  })

  it("orders selections the way the rows show them", () => {
    expect(
      orderLocationTokens([
        "sep",
        "token:spell",
        "leader:white",
        "main",
        "token",
        "leader:gold",
        "evolve",
      ]),
    ).toEqual(["main", "evolve", "leader:gold", "leader:white", "token", "token:spell", "sep"])
    expect(orderTypeTokens(["amulet", "crest", "follower"])).toEqual([
      "follower",
      "amulet",
      "crest",
    ])
  })
})
