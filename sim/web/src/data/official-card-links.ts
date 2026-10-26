import type { Catalog } from "./catalog"
import type { Row } from "./format-v3/decode"
import { arrayValue, objectValue, stringValue } from "./format-v3/json"

export type OfficialCardLink = {
  readonly region: "jp" | "en"
  readonly url: string
}

function officialUrl(printing: Row | undefined, region: "jp" | "en"): string | undefined {
  if (printing?.["catalog_state"] !== "official") return undefined
  const cardNo = stringValue(printing["card_no"])
  if (!cardNo) return undefined
  const base =
    region === "jp"
      ? "https://shadowverse-evolve.com/cardlist/"
      : "https://en.shadowverse-evolve.com/cards/"
  return `${base}?cardno=${encodeURIComponent(cardNo)}`
}

/** Return official pages in the selected printing's region order, using its exact catalog number. */
export function officialCardLinks(
  catalog: Catalog,
  cardId: string,
  printingId: string,
  region: "jp" | "en",
): readonly OfficialCardLink[] {
  const printings = catalog.index.printingsOf(cardId)
  const current = catalog.index.printing(printingId)
  const orderedRegions = [region, region === "jp" ? "en" : "jp"] as const
  const links: OfficialCardLink[] = []
  for (const targetRegion of orderedRegions) {
    const candidates = printings
      .filter(
        (printing) =>
          printing["region"] === targetRegion && printing["catalog_state"] === "official",
      )
      .sort((left, right) =>
        stringValue(left["card_no"]).localeCompare(stringValue(right["card_no"]), "en", {
          numeric: true,
        }),
      )
    const selected =
      targetRegion === region && current?.["region"] === region
        ? current
        : (candidates.find((printing) => {
            const card = catalog.index.card(cardId)
            return arrayValue(card?.["regions"] ?? []).some((rawView) => {
              const view = objectValue(rawView)
              return (
                view["region"] === targetRegion && view["default_printing_id"] === printing["id"]
              )
            })
          }) ?? candidates[0])
    const url = officialUrl(selected, targetRegion)
    if (url) links.push({ region: targetRegion, url })
  }
  return links
}
