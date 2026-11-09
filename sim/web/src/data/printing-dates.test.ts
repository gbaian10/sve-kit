// @vitest-environment node
import { expect, it } from "vitest"

import type { Row } from "./format-v3/decode"
import { printingReleaseDate } from "./printing-dates"

function snapshot(products: Row[], inclusions: Row[]) {
  return {
    bootstrap: [
      { table: "product", rows: products, file: "synthetic", identity: "product", value: {} },
      {
        table: "printing_product",
        rows: inclusions,
        file: "synthetic",
        identity: "printing_product",
        value: {},
      },
    ],
  }
}

it.each([
  ["2025-09-24", "day", "2025-09-24"],
  ["2025-09", "month", "2025-09"],
  [null, "unknown", undefined],
])("preserves manual release date %s with precision %s", (date, precision, expected) => {
  const printing = {
    id: "p:manual",
    catalog_state: "unlisted",
    manual_available_on: date,
    manual_date_precision: precision,
  }
  expect(
    printingReleaseDate(
      printing,
      snapshot(
        [{ id: "product", released_on: "2000-01-01", date_precision: "day" }],
        [{ printing_id: printing.id, product_id: "product", date_precision: null }],
      ),
    ),
  ).toBe(expected)
})

it("uses the earliest dated official inclusion and hides an uncertain first release", () => {
  const printing = {
    id: "p:official",
    catalog_state: "official",
    manual_available_on: "1900-01-01",
    manual_date_precision: "day",
  }
  const products = [
    { id: "first", released_on: "2024-01-01", date_precision: "day" },
    { id: "second", released_on: "2025-01-01", date_precision: "day" },
  ]
  const inclusions = [
    { printing_id: printing.id, product_id: "first", date_precision: null },
    {
      printing_id: printing.id,
      product_id: "second",
      available_on: "2023-12-01",
      date_precision: "day",
    },
  ]
  expect(printingReleaseDate(printing, snapshot(products, inclusions))).toBe("2023-12-01")
  expect(
    printingReleaseDate(
      printing,
      snapshot(products, [
        ...inclusions,
        { printing_id: printing.id, product_id: "unknown", date_precision: "unknown" },
      ]),
    ),
  ).toBeUndefined()
  expect(printingReleaseDate(printing, snapshot(products, []))).toBeUndefined()
})
