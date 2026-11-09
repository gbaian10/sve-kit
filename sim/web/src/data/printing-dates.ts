import type { LoadedSnapshot } from "./client"
import type { Row } from "./format-v3/decode"
import { mergedRows } from "./format-v3/reader"

export function manualReleaseDate(printing: Row | undefined): string | undefined {
  return printing?.["catalog_state"] === "unlisted" &&
    (printing["manual_date_precision"] === "day" ||
      printing["manual_date_precision"] === "month") &&
    typeof printing["manual_available_on"] === "string"
    ? printing["manual_available_on"]
    : undefined
}

/** An unknown inclusion prevents claiming a first release for an official printing. */
export function printingReleaseDate(
  printing: Row | undefined,
  snapshot: Pick<LoadedSnapshot, "bootstrap"> | null | undefined,
): string | undefined {
  if (!printing || printing["catalog_state"] === "unlisted") return manualReleaseDate(printing)
  const bootstrap = snapshot?.bootstrap ?? []
  const products = mergedRows(bootstrap, "product")
  const inclusions = mergedRows(bootstrap, "printing_product")
  const dates = inclusions
    .filter((row) => row["printing_id"] === printing["id"])
    .map((row) => {
      const product = products.find((product) => product["id"] === row["product_id"])
      const precision = row["date_precision"] ?? product?.["date_precision"]
      const date = row["date_precision"] == null ? product?.["released_on"] : row["available_on"]
      return precision === "day" && typeof date === "string" ? date : undefined
    })
  return dates.length && dates.every((date) => date !== undefined) ? dates.sort()[0] : undefined
}
