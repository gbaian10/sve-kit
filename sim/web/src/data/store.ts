import { drainCatalog } from "./catalog-cursor"
import type { LoadedSnapshot } from "./client"
import type { Row } from "./format-v3/decode"
import { integerValue, stringValue } from "./format-v3/json"
import { mergedRowsCursor } from "./format-v3/reader"

/** Lookups over loaded catalog and page fragments; absent detail remains unavailable. */
export interface CardIndex {
  readonly cards: readonly Row[]
  readonly families: readonly Row[]
  readonly card: (id: string) => Row | undefined
  readonly face: (id: string) => Row | undefined
  readonly facesOf: (cardId: string) => readonly Row[]
  readonly printing: (id: string) => Row | undefined
  readonly printingsOf: (cardId: string) => readonly Row[]
  readonly printingByCardNo: (region: string, cardNo: string) => Row | undefined
  /** The bootstrap columns of the current revision for one face in one region. */
  readonly currentRevision: (faceId: string, region: string) => Row | undefined
  readonly revision: (id: string) => Row | undefined
  /** Current takes precedence; a pending display is only a presentation value. */
  readonly displayRevision: (faceId: string, region: string, printingId?: string) => Row | undefined
  readonly wording: (faceId: string, region: string) => Row | undefined
  readonly family: (id: string) => Row | undefined
  readonly product: (id: string) => Row | undefined
  readonly vocabulary: (kind: string, code: string) => Row | undefined
  readonly keyword: (id: string) => Row | undefined
  readonly support: (cardId: string) => Row | undefined
  /** Text units shipped in the bootstrap (names, labels); others come from detail files. */
  readonly textUnit: (id: string) => Row | undefined
  readonly translation: (id: string) => Row | undefined
}

function* byId(rows: readonly Row[], field = "id"): Generator<void, Map<string, Row>> {
  const out = new Map<string, Row>()
  for (const row of rows) {
    out.set(stringValue(row[field]), row)
    yield
  }
  return out
}
function* group(rows: readonly Row[], field: string): Generator<void, Map<string, Row[]>> {
  const out = new Map<string, Row[]>()
  for (const row of rows) {
    const key = stringValue(row[field])
    const list = out.get(key)
    if (list) list.push(row)
    else out.set(key, [row])
    yield
  }
  return out
}
export function createCardIndex(snapshot: LoadedSnapshot): CardIndex {
  return drainCatalog(cardIndexCursor(snapshot))
}

export function* cardIndexCursor(snapshot: LoadedSnapshot): Generator<void, CardIndex> {
  const cards = yield* mergedRowsCursor(snapshot.bootstrap, "card")
  const faces = yield* mergedRowsCursor(snapshot.bootstrap, "face")
  const printingOwners = new Map<string, Row[string]>()
  for (const fragment of snapshot.bootstrap)
    if (fragment.table === "printing")
      for (const row of fragment.rows) {
        printingOwners.set(stringValue(row["id"]), (fragment.value["owner"] as Row)["id"] ?? null)
        yield
      }
  const printings: Row[] = [],
    printingIds = new Set<string>()
  for (const row of yield* mergedRowsCursor(snapshot.bootstrap, "printing")) {
    printings.push({ ...row, home_set_id: printingOwners.get(stringValue(row["id"])) ?? null })
    printingIds.add(stringValue(row["id"]))
    yield
  }
  for (const fragment of snapshot.bootstrap)
    if (fragment.table === "card_locator")
      for (const row of fragment.rows) {
        if (!printingIds.has(stringValue(row["id"]))) printings.push(row)
        yield
      }
  const revisions = yield* mergedRowsCursor(snapshot.bootstrap, "face_revision")
  const families = yield* mergedRowsCursor(snapshot.bootstrap, "product_family")
  const cardMap = yield* byId(cards)
  const faceMap = yield* byId(faces)
  const facesByCard = yield* group(faces, "card_id")
  for (const list of facesByCard.values()) {
    list.sort((a, b) => integerValue(a["ordinal"]) - integerValue(b["ordinal"]))
    yield
  }
  const printingMap = yield* byId(printings)
  const printingsByCard = yield* group(printings, "card_id")
  const printingByNo = new Map<string, Row>()
  for (const row of printings) {
    printingByNo.set(`${stringValue(row["region"])}\u0000${stringValue(row["card_no"])}`, row)
    yield
  }
  const revisionMap = yield* byId(revisions)
  const familyMap = yield* byId(families)
  const productMap = yield* byId(yield* mergedRowsCursor(snapshot.bootstrap, "product"))
  const vocabularyMap = new Map<string, Row>()
  for (const row of yield* mergedRowsCursor(snapshot.bootstrap, "vocabulary")) {
    vocabularyMap.set(`${stringValue(row["kind"])}\u0000${stringValue(row["code"])}`, row)
    yield
  }
  const keywordMap = yield* byId(yield* mergedRowsCursor(snapshot.bootstrap, "keyword"))
  const supportMap = yield* byId(
    yield* mergedRowsCursor(snapshot.bootstrap, "card_engine_support"),
    "card_id",
  )
  const textMap = yield* byId(yield* mergedRowsCursor(snapshot.bootstrap, "text_unit"))
  const translationMap = yield* byId(yield* mergedRowsCursor(snapshot.bootstrap, "translation"))
  const currentRevision = (faceId: string, region: string): Row | undefined => {
    const face = faceMap.get(faceId)
    for (const entry of (face?.["current"] ?? []) as Row[]) {
      if (entry["region"] === region) return revisionMap.get(stringValue(entry["revision_id"]))
    }
    return undefined
  }
  const wording = (faceId: string, region: string): Row | undefined =>
    ((faceMap.get(faceId)?.["wording"] ?? []) as Row[]).find((entry) => entry["region"] === region)
  return {
    cards,
    families,
    card: (id) => cardMap.get(id),
    face: (id) => faceMap.get(id),
    facesOf: (cardId) => facesByCard.get(cardId) ?? [],
    printing: (id) => printingMap.get(id),
    printingsOf: (cardId) => printingsByCard.get(cardId) ?? [],
    printingByCardNo: (region, cardNo) => printingByNo.get(`${region}\u0000${cardNo}`),
    currentRevision,
    revision: (id) => revisionMap.get(id),
    wording,
    displayRevision: (faceId, region, printingId) => {
      if (printingId !== undefined) {
        const own = ((wording(faceId, region)?.["candidates"] ?? []) as Row[]).filter(
          (candidate) => candidate["printing_id"] === printingId,
        )
        if (own.length === 1 && typeof own[0]?.["revision_id"] === "string")
          return revisionMap.get(own[0]["revision_id"])
        if (own.length > 0) return undefined
      }
      const current = currentRevision(faceId, region)
      if (current) return current
      const display = wording(faceId, region)?.["display"] as Row | undefined
      return typeof display?.["revision_id"] === "string"
        ? revisionMap.get(display["revision_id"])
        : undefined
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
