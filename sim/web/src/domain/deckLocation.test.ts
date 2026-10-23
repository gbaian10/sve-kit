import { describe, expect, it } from "vitest"

import {
  deckLocationBadges,
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
    ).toEqual(["main", "evolve", "token", "token:spell", "leader:gold", "leader:white", "sep"])
    expect(orderTypeTokens(["amulet", "crest", "follower"])).toEqual([
      "follower",
      "amulet",
      "crest",
    ])
  })
})

it("keeps overlapping card-face positions in their order and omits main deck, EP and SEP", () => {
  expect(deckLocationBadges("follower", ["token", "advance", "evolve"])).toEqual([
    "evolve",
    "advance",
    "token",
  ])
  expect(deckLocationBadges("leader", [])).toEqual(["leader"])
  expect(deckLocationBadges("leader", ["token"])).toEqual(["leader", "token"])
  for (const type of ["follower", "ep", "sep"]) expect(deckLocationBadges(type, [])).toEqual([])
})
