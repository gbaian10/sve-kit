import type { LoadedSnapshot, SnapshotClient } from "./client"
import type { Row } from "./format-v1/decode"
import { SnapshotError } from "./format-v1/errors"
import { integerValue, objectValue, stringValue } from "./format-v1/json"
import { bucketOf, createLocator, GLOBAL_OWNER } from "./locator"

export interface ImageSource {
  readonly src: string
  readonly srcSet: string
  readonly width: number
  readonly height: number
}
export interface ImageFace {
  readonly printingId: string
  readonly faceId: string
}
export interface ImageIndex {
  readonly cardImage: (printingId: string, faceId: string) => ImageSource | undefined
  readonly asset: (printingId: string, faceId: string) => Row | undefined
  readonly known?: (printingId: string, faceId: string) => boolean
  readonly failed?: boolean
}
const CARD_SIZES = ["card_s", "card_m", "card_l"]
const faceKey = (printingId: string, faceId: string) => `${printingId}\u0000${faceId}`

export function imageSource(base: string, variants: readonly Row[]): ImageSource | undefined {
  const cards = variants
    .filter((row) => CARD_SIZES.includes(stringValue(row["size_key"])))
    .sort((a, b) => integerValue(a["width"]) - integerValue(b["width"]))
  const largest = cards.at(-1)
  if (!largest) return undefined
  return {
    src: `${base}/${stringValue(largest["path"])}`,
    srcSet: cards
      .map((row) => `${base}/${stringValue(row["path"])} ${String(integerValue(row["width"]))}w`)
      .join(", "),
    width: integerValue(largest["width"]),
    height: integerValue(largest["height"]),
  }
}

/** Parse only the page's files, retaining only its binding, asset and card-size rows. */
async function readImageFaces(
  client: SnapshotClient,
  faces: readonly ImageFace[],
  signal?: AbortSignal,
): Promise<ImageIndex> {
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("snapshot not loaded")
  const check = () => {
    if (signal?.aborted || client.snapshot() !== snapshot) throw new Error("image page cancelled")
  }
  const locate = createLocator(
    snapshot.files,
    new Set(["printing_image", "image_asset", "image_variant"]),
  )
  const requestedPrintings = new Set(faces.map((face) => face.printingId))
  const count = integerValue(objectValue(snapshot.manifest["partitioning"])["bucket_count"])
  const neededFaces = new Set(faces.map((face) => faceKey(face.printingId, face.faceId)))
  const keys = new Set<string>()
  for (const fragment of snapshot.bootstrap) {
    if (fragment.table !== "printing") continue
    for (const row of fragment.rows) {
      const id = stringValue(row["id"])
      if (!requestedPrintings.has(id)) continue
      const key = locate({
        table: "printing_image",
        owner: objectValue(fragment.value["owner"]),
        bucket: bucketOf([id], count),
        partition: "detail",
      })
      if (key) keys.add(key)
    }
  }
  const bindings = new Map<string, string>()
  const assets = new Map<string, Row>()
  const variants = new Map<string, Row[]>()
  let raw = 0
  const read = async (
    key: string,
    consume: (fragments: Awaited<ReturnType<SnapshotClient["fragments"]>>) => void,
  ) => {
    check()
    raw += integerValue(snapshot.files.get(key)?.["bytes"])
    if (count === 64 && raw > 12 * 1024 * 1024)
      throw new SnapshotError("payload-set", "image page exceeds 12 MiB; reduce visible faces")
    const fragments = await client.fragments(key)
    check()
    consume(fragments)
  }
  const ids = new Set<string>()
  const keep = (table: string, row: Row) => {
    if (table === "image_asset" && ids.has(stringValue(row["id"])))
      assets.set(stringValue(row["id"]), row)
    if (
      table === "image_variant" &&
      ids.has(stringValue(row["image_id"])) &&
      CARD_SIZES.includes(stringValue(row["size_key"]))
    ) {
      const id = stringValue(row["image_id"])
      variants.set(id, [...(variants.get(id) ?? []), row])
    }
  }
  // Sequential parsing limits temporary decoded heap; the network queue still prioritizes this page.
  for (const key of keys)
    await read(key, (fragments) => {
      for (const fragment of fragments) {
        if (fragment.table !== "printing_image") continue
        for (const row of fragment.rows) {
          const face = faceKey(stringValue(row["printing_id"]), stringValue(row["face_id"]))
          if (neededFaces.has(face)) {
            const id = stringValue(row["image_id"])
            bindings.set(face, id)
            ids.add(id)
          }
        }
      }
      // 1.0 holds bindings and asset rows in the same file.
      for (const fragment of fragments) for (const row of fragment.rows) keep(fragment.table, row)
    })
  const global = new Set<string>()
  for (const id of ids)
    for (const table of ["image_asset", "image_variant"]) {
      const key = locate({
        table,
        owner: GLOBAL_OWNER,
        bucket: bucketOf([id], count),
        partition: "detail",
      })
      if (key && !keys.has(key)) global.add(key)
    }
  for (const key of global)
    await read(key, (fragments) => {
      for (const fragment of fragments) for (const row of fragment.rows) keep(fragment.table, row)
    })
  check()
  for (const id of ids) {
    const asset = assets.get(id)
    if (!asset) throw new SnapshotError("dangling-reference", "image binding has no asset")
    if (
      (variants.get(id)?.length ?? 0) > 0 &&
      (asset["publication_state"] !== "approved" || asset["availability"] !== "available")
    )
      throw new SnapshotError("image-variant-unapproved", "variants for unpublished asset")
  }
  return {
    asset: (printingId, faceId) => assets.get(bindings.get(faceKey(printingId, faceId)) ?? ""),
    cardImage: (printingId, faceId) => {
      const id = bindings.get(faceKey(printingId, faceId))
      const asset = assets.get(id ?? "")
      return asset?.["publication_state"] === "approved" && asset["availability"] === "available"
        ? imageSource(client.base, variants.get(id ?? "") ?? [])
        : undefined
    },
  }
}

interface FaceImage {
  readonly asset: Row | undefined
  readonly source: ImageSource | undefined
}
const faceCaches = new WeakMap<LoadedSnapshot, Map<string, FaceImage>>()
const legacyIndexes = new WeakMap<LoadedSnapshot, Promise<ImageIndex>>()
const MAX_FACES = 64

function indexOf(rows: ReadonlyMap<string, FaceImage>): ImageIndex {
  return {
    known: (printingId, faceId) => rows.has(faceKey(printingId, faceId)),
    asset: (printingId, faceId) => rows.get(faceKey(printingId, faceId))?.asset,
    cardImage: (printingId, faceId) => rows.get(faceKey(printingId, faceId))?.source,
  }
}

async function legacyIndex(client: SnapshotClient, snapshot: LoadedSnapshot): Promise<ImageIndex> {
  const assets = new Map<string, Row>()
  const variants = new Map<string, Row[]>()
  const bindings = new Map<string, string>()
  for (const [key, file] of snapshot.files) {
    if (file["role"] !== "images") continue
    if (client.snapshot() !== snapshot) throw new Error("image page cancelled")
    for (const fragment of await client.fragments(key))
      for (const row of fragment.rows) {
        if (fragment.table === "image_asset") assets.set(stringValue(row["id"]), row)
        if (
          fragment.table === "image_variant" &&
          CARD_SIZES.includes(stringValue(row["size_key"]))
        ) {
          const id = stringValue(row["image_id"])
          const list = variants.get(id) ?? []
          list.push(row)
          variants.set(id, list)
        }
        if (fragment.table === "printing_image")
          bindings.set(
            faceKey(stringValue(row["printing_id"]), stringValue(row["face_id"])),
            stringValue(row["image_id"]),
          )
      }
  }
  const sources = new Map<string, ImageSource>()
  for (const [id, rows] of variants) {
    const asset = assets.get(id)
    if (asset?.["publication_state"] !== "approved" || asset["availability"] !== "available")
      throw new SnapshotError("image-variant-unapproved", "variants for unpublished asset")
    const source = imageSource(client.base, rows)
    if (source) sources.set(id, source)
  }
  for (const id of bindings.values())
    if (!assets.has(id)) throw new SnapshotError("dangling-reference", "image binding has no asset")
  // The unsharded legacy file is derived once, not re-decoded on every viewport change.
  return {
    known: () => true,
    asset: (printingId, faceId) => assets.get(bindings.get(faceKey(printingId, faceId)) ?? ""),
    cardImage: (printingId, faceId) => sources.get(bindings.get(faceKey(printingId, faceId)) ?? ""),
  }
}

/** Keep overlapping faces during scrolling; only missing faces need new fragment decoding. */
export async function loadImagePage(
  client: SnapshotClient,
  faces: readonly ImageFace[],
  signal?: AbortSignal,
): Promise<ImageIndex> {
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("snapshot not loaded")
  const check = () => {
    if (signal?.aborted || client.snapshot() !== snapshot) throw new Error("image page cancelled")
  }
  check()
  if (snapshot.manifest["format_version"] === "1.0.0") {
    let pending = legacyIndexes.get(snapshot)
    if (!pending) {
      pending = legacyIndex(client, snapshot).catch((error: unknown) => {
        legacyIndexes.delete(snapshot)
        throw error
      })
      legacyIndexes.set(snapshot, pending)
    }
    const index = await pending
    check()
    return index
  }
  let cache = faceCaches.get(snapshot)
  if (!cache) {
    cache = new Map()
    faceCaches.set(snapshot, cache)
  }
  const missing = faces.filter((face) => !cache.has(faceKey(face.printingId, face.faceId)))
  if (missing.length) {
    const loaded = await readImageFaces(client, missing, signal)
    check()
    for (const face of missing)
      cache.set(faceKey(face.printingId, face.faceId), {
        asset: loaded.asset(face.printingId, face.faceId),
        source: loaded.cardImage(face.printingId, face.faceId),
      })
  }
  const selected = new Map<string, FaceImage>()
  for (const face of faces) {
    const key = faceKey(face.printingId, face.faceId)
    const entry = cache.get(key)
    if (!entry) continue
    selected.set(key, entry)
    cache.delete(key)
    cache.set(key, entry)
  }
  while (cache.size > MAX_FACES) {
    const oldest = cache.keys().next().value
    if (oldest === undefined) break
    cache.delete(oldest)
  }
  check()
  return indexOf(selected)
}
