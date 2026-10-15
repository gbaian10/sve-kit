import { NORMALIZER_VERSION } from "../../domain/normalize"
import { VIEW_BUDGET } from "../cache-budget"
import type { Row } from "../format-v3/decode"
import { decodeMessage } from "../format-v3/decode-message"
import {
  arrayValue,
  canonicalText,
  integerValue,
  type JsonObject,
  objectValue,
  parseJson,
  stringValue,
} from "../format-v3/json"
import type { Files } from "../format-v3/reader"
import { mediaImageSource } from "../image-url"
import type { ImageIndex, ImageSource } from "../images"
import { createLocator, homeSetOwner } from "../locator"
import { VerifiedCache } from "../verified-cache"
import type { SearchBlobs } from "./blobs"
import type { SearchIndex, SearchOptions, SearchPage, SearchQuery } from "./index"

const VIEW_FORMAT = 3
const REGION_POLICY = "selected-region"
export interface ViewFace {
  readonly printingId: string
  readonly faceId: string
  readonly intId: number
  readonly ordinal: number
  readonly asset: Row
  readonly source?: ImageSource
  readonly artSource?: ImageSource
}
export interface CompleteView {
  readonly query: SearchQuery
  readonly page: SearchPage
  readonly options: SearchOptions
  readonly faces: readonly ViewFace[]
}
export const viewSlot = (query: SearchQuery): string =>
  canonicalText([
    REGION_POLICY,
    NORMALIZER_VERSION,
    { ...query, offset: query.offset ?? 0, limit: query.limit ?? 24 },
  ] as unknown as import("../format-v3/json").JsonValue)

export function viewImages(view: CompleteView): ImageIndex {
  const faces = new Map(view.faces.map((face) => [`${face.printingId}\u0000${face.faceId}`, face]))
  const get = (printing: string, face: string) => faces.get(`${printing}\u0000${face}`)
  return {
    known: (printing, face) => get(printing, face) !== undefined,
    asset: (printing, face) => get(printing, face)?.asset,
    cardImage: (printing, face) => get(printing, face)?.source,
    artImage: (printing, face) => get(printing, face)?.artSource,
  }
}
export class ViewCache {
  private readonly stored: VerifiedCache
  private readonly current: VerifiedCache
  constructor(root: string, storage?: CacheStorage, build?: string) {
    this.stored = new VerifiedCache(root, "search-view", VIEW_FORMAT, storage, build)
    this.current = new VerifiedCache(root, "search-view-selection", VIEW_FORMAT, storage, build)
  }
  async readCurrent(
    edition: SearchQuery["edition"],
    hash: string,
  ): Promise<SearchQuery | undefined> {
    const bytes = await this.current.read(edition, hash)
    if (!bytes) return undefined
    try {
      const query = objectValue(parseJson(bytes)) as unknown as SearchQuery
      return query.edition === edition ? query : undefined
    } catch {
      return undefined
    }
  }
  async remember(query: SearchQuery, hash: string): Promise<void> {
    await this.current.write(query.edition, hash, new TextEncoder().encode(JSON.stringify(query)))
  }
  async read(query: SearchQuery, hash: string): Promise<CompleteView | undefined> {
    const bytes = await this.stored.read(viewSlot(query), hash)
    if (!bytes) return undefined
    try {
      const record = objectValue(parseJson(bytes))
      if (!Array.isArray(record["faces"])) return undefined
      const view = record as unknown as CompleteView
      if (viewSlot(view.query) !== viewSlot(query)) return undefined
      return view
    } catch {
      return undefined
    }
  }
  async write(view: CompleteView, hash: string): Promise<void> {
    const bytes = new TextEncoder().encode(JSON.stringify(view))
    if (bytes.length <= VIEW_BUDGET) await this.stored.write(viewSlot(view.query), hash, bytes)
  }
}

export function completeView(
  _root: string,
  query: SearchQuery,
  index: SearchIndex,
  _files: Files,
  _version: string,
  _blobs: SearchBlobs,
  _signal: AbortSignal,
  eligible?: ReadonlySet<string>,
): Promise<CompleteView> {
  const page = index.search(query, eligible)
  return Promise.resolve({ query, page, faces: [], options: index.options })
}

export async function completeImages(
  root: string,
  printingIds: readonly string[],
  index: SearchIndex,
  files: Files,
  version: string,
  blobs: SearchBlobs,
  signal: AbortSignal,
): Promise<readonly ViewFace[]> {
  const targets = index.imageTargets(printingIds)
  const locate = createLocator(files, new Set(["printing_image"]))
  const keys = new Set(
    targets.map((target) =>
      locate({
        table: "printing_image",
        owner: homeSetOwner(target.owner),
        entityKey: [target.printingId],
        partition: "detail",
      }),
    ),
  )
  if (keys.has(undefined)) throw new Error("image metadata locator missing")
  const rows = new Map<string, JsonObject>()
  const needed = [...keys].map((key) => files.get(key ?? ""))
  if (needed.reduce((n, file) => n + integerValue(file?.["bytes"]), 0) > 12 * 1024 * 1024)
    throw new Error("image view exceeds 12 MiB")
  await Promise.all(
    needed.map(async (file) => {
      if (!file) throw new Error("image metadata file missing")
      const bytes = await blobs.read(file, signal)
      const decoded = decodeMessage({ kind: "file", bytes, file, version })
      if (decoded.kind !== "fragments") throw new Error("image metadata shape")
      for (const fragment of decoded.fragments)
        if (fragment.table === "printing_image")
          for (const row of fragment.rows)
            rows.set(`${stringValue(row["printing_id"])}\u0000${stringValue(row["face_id"])}`, row)
    }),
  )
  signal.throwIfAborted()
  const integers = new Map<string, number>()
  const baseKeys = new Set<string>()
  for (const target of targets) {
    if (target.intId) continue
    for (const file of needed)
      for (const raw of arrayValue(file?.["dependencies"] ?? [])) {
        const ref = objectValue(raw)
        const key = stringValue(ref["key"])
        const base = files.get(key)
        if (base && arrayValue(base["row_counts"]).some((c) => arrayValue(c)[0] === "printing")) {
          if (base["sha256"] !== ref["sha256"]) throw new Error("printing exact dependency hash")
          baseKeys.add(key)
        }
      }
  }
  await Promise.all(
    [...baseKeys].map(async (key) => {
      const file = files.get(key)
      if (!file) throw new Error("printing base missing")
      const decoded = decodeMessage({
        kind: "file",
        file,
        bytes: await blobs.read(file, signal),
        version,
      })
      if (decoded.kind !== "fragments") throw new Error("printing base shape")
      for (const fragment of decoded.fragments)
        if (fragment.table === "printing")
          for (const row of fragment.rows)
            integers.set(stringValue(row["id"]), integerValue(row["int_id"]))
    }),
  )
  const faces = targets.map((target) => {
    const intId = target.intId || integers.get(target.printingId)
    if (!intId) throw new Error("printing image identity missing")
    const row = rows.get(`${target.printingId}\u0000${target.faceId}`)
    if (!row) throw new Error("image metadata face missing")
    const asset = {
      rarity_code: target.rarityCode ?? null,
      publication_state: row["publication_state"] ?? null,
      availability: row["availability"] ?? null,
      card_version: row["card_version"] ?? null,
      art_version: row["art_version"] ?? null,
      variants: row["variants"] ?? [],
    }
    const source = mediaImageSource(root, intId, target.ordinal, asset, "card")
    const artSource = mediaImageSource(root, intId, target.ordinal, asset, "art")
    if (!source && row["publication_state"] === "approved" && row["availability"] === "available")
      throw new Error("available image has no variants")
    return {
      ...target,
      intId,
      asset,
      ...(source ? { source } : {}),
      ...(artSource ? { artSource } : {}),
    }
  })
  return faces
}
