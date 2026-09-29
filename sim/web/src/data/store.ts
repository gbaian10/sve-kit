import type { LoadedSnapshot } from "./client"
import type { Row } from "./format-v1/decode"
import { integerValue, stringValue } from "./format-v1/json"

/** Bootstrap-only lookups every page needs; detail rows are fetched through the client. */
export interface CardIndex {
  readonly cards: readonly Row[]
  readonly families: readonly Row[]
  readonly keywords: readonly Row[]
  readonly mechanicProjections: readonly Row[]
  readonly mechanicCoverage: readonly Row[]
  readonly printingProducts: readonly Row[]
  readonly card: (id: string) => Row | undefined
  readonly face: (id: string) => Row | undefined
  readonly facesOf: (cardId: string) => readonly Row[]
  readonly printing: (id: string) => Row | undefined
  readonly printingsOf: (cardId: string) => readonly Row[]
  readonly printingByCardNo: (region: string, cardNo: string) => Row | undefined
  /** Any region's printing with this raw card number (numbers are unique across regions). */
  readonly printingByAnyCardNo: (cardNo: string) => Row | undefined
  readonly printingByIntId: (intId: number) => Row | undefined
  /** The bootstrap columns of the current revision for one face in one region. */
  readonly currentRevision: (faceId: string, region: string) => Row | undefined
  readonly family: (id: string) => Row | undefined
  readonly product: (id: string) => Row | undefined
  readonly vocabulary: (kind: string, code: string) => Row | undefined
  readonly keyword: (id: string) => Row | undefined
  readonly support: (cardId: string) => Row | undefined
  /** Text units shipped in the bootstrap (names, labels); others come from detail files. */
  readonly textUnit: (id: string) => Row | undefined
  readonly translation: (id: string) => Row | undefined
}

function byId(rows: readonly Row[], field = "id"): Map<string, Row> {
  return new Map(rows.map((row) => [stringValue(row[field]), row]))
}

function group(rows: readonly Row[], field: string): Map<string, Row[]> {
  const out = new Map<string, Row[]>()
  for (const row of rows) {
    const key = stringValue(row[field])
    out.set(key, [...(out.get(key) ?? []), row])
  }
  return out
}

export function createCardIndex(snapshot: LoadedSnapshot): CardIndex {
  const rows = (table: string): Row[] =>
    snapshot.bootstrap
      .filter((fragment) => fragment.table === table)
      .flatMap((fragment) => fragment.rows)
  const cards = rows("card")
  const faces = rows("face")
  const printings = rows("printing")
  const revisions = rows("face_revision")
  const families = rows("product_family")
  const cardMap = byId(cards)
  const faceMap = byId(faces)
  const facesByCard = group(faces, "card_id")
  for (const list of facesByCard.values())
    list.sort((a, b) => integerValue(a["ordinal"]) - integerValue(b["ordinal"]))
  const printingMap = byId(printings)
  const printingsByCard = group(printings, "card_id")
  const printingByNo = new Map(
    printings.map((row) => [
      `${stringValue(row["region"])}\u0000${stringValue(row["card_no"])}`,
      row,
    ]),
  )
  const printingByAnyNo = new Map(printings.map((row) => [stringValue(row["card_no"]), row]))
  const printingByInt = new Map(printings.map((row) => [integerValue(row["int_id"]), row]))
  const revisionMap = byId(revisions)
  const familyMap = byId(families)
  const productMap = byId(rows("product"))
  const vocabularyMap = new Map(
    rows("vocabulary").map((row) => [
      `${stringValue(row["kind"])}\u0000${stringValue(row["code"])}`,
      row,
    ]),
  )
  const keywords = rows("keyword")
  const mechanicProjections = rows("mechanic_projection")
  const mechanicCoverage = rows("card_mechanic_coverage")
  const printingProducts = rows("printing_product")
  const keywordMap = byId(keywords)
  const supportMap = byId(rows("card_engine_support"), "card_id")
  const textMap = byId(rows("text_unit"))
  const translationMap = byId(rows("translation"))
  return {
    cards,
    families,
    keywords,
    mechanicProjections,
    mechanicCoverage,
    printingProducts,
    card: (id) => cardMap.get(id),
    face: (id) => faceMap.get(id),
    facesOf: (cardId) => facesByCard.get(cardId) ?? [],
    printing: (id) => printingMap.get(id),
    printingsOf: (cardId) => printingsByCard.get(cardId) ?? [],
    printingByCardNo: (region, cardNo) => printingByNo.get(`${region}\u0000${cardNo}`),
    printingByAnyCardNo: (cardNo) => printingByAnyNo.get(cardNo),
    printingByIntId: (intId) => printingByInt.get(intId),
    currentRevision: (faceId, region) => {
      const face = faceMap.get(faceId)
      if (!face) return undefined
      for (const entry of face["current"] as Row[]) {
        if (entry["region"] === region) return revisionMap.get(stringValue(entry["revision_id"]))
      }
      return undefined
    },
    family: (id) => familyMap.get(id),
    product: (id) => productMap.get(id),
    vocabulary: (kind, code) => vocabularyMap.get(`${kind}\u0000${code}`),
    keyword: (id) => keywordMap.get(id),
    support: (cardId) => supportMap.get(cardId),
    textUnit: (id) => textMap.get(id),
    translation: (id) => translationMap.get(id),
  }
}
