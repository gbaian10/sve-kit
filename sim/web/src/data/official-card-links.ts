import type { Catalog } from "./catalog"
import type { Row } from "./format-v3/decode"
import { arrayValue, objectValue, stringValue } from "./format-v3/json"

export type OfficialCardLink = {
  readonly region: "jp" | "en"
  readonly url: string
}

const OFFICIAL_PREFIXES = [
  "https://shadowverse-evolve.com/",
  "https://en.shadowverse-evolve.com/",
] as const
const OFFICIAL_ORIGINS = new Set(OFFICIAL_PREFIXES.map((prefix) => new URL(prefix).origin))

function officialUrl(value: unknown): string | undefined {
  if (typeof value !== "string" || !OFFICIAL_PREFIXES.some((prefix) => value.startsWith(prefix)))
    return undefined
  try {
    const url = new URL(value)
    return OFFICIAL_ORIGINS.has(url.origin) ? value : undefined
  } catch {
    return undefined
  }
}

function sourceUrl(printing: Row | undefined): string | undefined {
  for (const rawFace of arrayValue(printing?.["faces"] ?? [])) {
    for (const rawObservation of arrayValue(objectValue(rawFace)["observations"] ?? [])) {
      const url = officialUrl(objectValue(rawObservation)["source_url"])
      if (url) return url
    }
  }
  return undefined
}

/** Return official pages in the selected printing's region order, using observed snapshot URLs. */
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
      .filter((printing) => printing["region"] === targetRegion)
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
    const url = sourceUrl(selected)
    if (url) links.push({ region: targetRegion, url })
  }
  return links
}
