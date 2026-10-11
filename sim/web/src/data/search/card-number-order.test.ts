// @vitest-environment node
import { expect, it } from "vitest"

import { cardNumberOrder } from "./card-number-order"

it("orders set numbers naturally, then prefix, integer number, suffix and permanent identities", () => {
  const lookups = [
    "bp10|1|",
    "bp2|1|",
    "bp01|p10|",
    "bp01|p2|",
    "bp01|2|a",
    "bp01|10|",
    "bp01|2|",
    "bp01|ld1|",
    "bp01|sl1|",
    "bp01|pr1|",
    "bp01|2|en",
  ]
  const ranks = cardNumberOrder(
    lookups.map((lookup, i) => ({
      lookup,
      flat: lookup,
      cardId: `c:${String(i).padStart(2, "0")}`,
      printingId: `p:${String(i)}`,
    })),
  )
  expect([...ranks]).toEqual([10, 9, 6, 5, 2, 3, 0, 4, 8, 7, 1])
})

it("equivalent normalized lookup keys tie by identity, while unknown forms follow known numbers", () => {
  const rows = [
    { lookup: "bp01|1|", flat: "bp01001", cardId: "c:b", printingId: "p:b" },
    { lookup: "bp01|1|", flat: "bp011", cardId: "c:a", printingId: "p:a" },
    { lookup: "", flat: "unknown", cardId: "c:0", printingId: "p:0" },
  ]
  expect([...cardNumberOrder(rows)]).toEqual([1, 0, 2])
})
