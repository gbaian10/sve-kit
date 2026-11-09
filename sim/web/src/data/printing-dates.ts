import type { LoadedSnapshot } from "./client"
import type { Row } from "./format-v3/decode"
import { type Fragment, mergedRows } from "./format-v3/reader"

export function manualReleaseDate(printing: Row | undefined): string | undefined {
  return printing?.["catalog_state"] === "unlisted" &&
    (printing["manual_date_precision"] === "day" ||
      printing["manual_date_precision"] === "month") &&
    typeof printing["manual_available_on"] === "string"
    ? printing["manual_available_on"]
    : undefined
}

// A snapshot replaces its bootstrap array instead of mutating it, so the array identifies its rows.
const inclusionDates = new WeakMap<
  readonly Fragment[],
  ReadonlyMap<string, readonly (string | undefined)[]>
>()

/** Merging every inclusion costs tens of milliseconds, too much to repeat on each card page render. */
function inclusionDatesOf(
  bootstrap: readonly Fragment[],
): ReadonlyMap<string, readonly (string | undefined)[]> {
  const cached = inclusionDates.get(bootstrap)
  if (cached) return cached
  const products = new Map(mergedRows(bootstrap, "product").map((row) => [row["id"], row]))
  const dates = new Map<string, (string | undefined)[]>()
  for (const row of mergedRows(bootstrap, "printing_product")) {
    if (typeof row["printing_id"] !== "string") continue
    const product = products.get(row["product_id"])
    const precision = row["date_precision"] ?? product?.["date_precision"]
    const date = row["date_precision"] == null ? product?.["released_on"] : row["available_on"]
    const list = dates.get(row["printing_id"]) ?? []
    list.push(precision === "day" && typeof date === "string" ? date : undefined)
    dates.set(row["printing_id"], list)
  }
  inclusionDates.set(bootstrap, dates)
  return dates
}

/** An unknown inclusion prevents claiming a first release for an official printing. */
export function printingReleaseDate(
  printing: Row | undefined,
  snapshot: Pick<LoadedSnapshot, "bootstrap"> | null | undefined,
): string | undefined {
  if (!printing || printing["catalog_state"] === "unlisted") return manualReleaseDate(printing)
  const id = printing["id"]
  const bootstrap = snapshot?.bootstrap
  const dates = bootstrap && typeof id === "string" ? inclusionDatesOf(bootstrap).get(id) : []
  return dates?.length && dates.every((date) => date !== undefined)
    ? [...dates].sort()[0]
    : undefined
}
