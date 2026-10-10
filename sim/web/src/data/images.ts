import type { LoadedSnapshot, SnapshotClient } from "./client"
import type { Row } from "./format-v3/decode"
import { SnapshotError } from "./format-v3/errors"
import { arrayValue, integerValue, objectValue, stringValue } from "./format-v3/json"
import { validateMedia } from "./format-v3/media"
import { mediaImageSource } from "./image-url"
import { bucketOf, createLocator } from "./locator"

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
  readonly eager?: (printingId: string, faceId: string) => boolean
  readonly failed?: boolean
}
const faceKey = (printingId: string, faceId: string) => `${printingId}\u0000${faceId}`

interface MediaLookup {
  readonly printings: ReadonlyMap<string, { readonly row: Row; readonly owner: Row }>
  readonly faces: ReadonlyMap<string, Row>
  readonly locate: ReturnType<typeof createLocator>
}
const mediaLookups = new WeakMap<LoadedSnapshot, MediaLookup>()

function mediaLookup(snapshot: LoadedSnapshot): MediaLookup {
  const existing = mediaLookups.get(snapshot)
  if (existing) return existing
  const printings = new Map<string, { readonly row: Row; readonly owner: Row }>()
  const faces = new Map<string, Row>()
  for (const fragment of snapshot.bootstrap) {
    if (fragment.table !== "printing" && fragment.table !== "face") continue
    for (const row of fragment.rows) {
      const id = stringValue(row["id"])
      const duplicate = fragment.table === "printing" ? printings.has(id) : faces.has(id)
      if (duplicate)
        throw new SnapshotError("primary-key-duplicate", "duplicate media bootstrap target")
      if (fragment.table === "printing")
        printings.set(id, { row, owner: objectValue(fragment.value["owner"]) })
      else faces.set(id, row)
    }
  }
  const lookup = {
    printings,
    faces,
    locate: createLocator(snapshot.files, new Set(["printing_image"])),
  }
  mediaLookups.set(snapshot, lookup)
  return lookup
}

async function readMediaFaces(
  client: SnapshotClient,
  faces: readonly ImageFace[],
  signal?: AbortSignal,
): Promise<ImageIndex> {
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("snapshot not loaded")
  const check = () => {
    if (signal?.aborted || client.snapshot() !== snapshot) throw new Error("image page cancelled")
  }
  const lookup = mediaLookup(snapshot)
  const printings = new Map<string, Row>()
  const faceRows = new Map<string, Row>()
  const keys = new Set<string>()
  for (const { printingId, faceId } of faces) {
    const face = lookup.faces.get(faceId)
    if (face) faceRows.set(faceId, face)
    const printing = lookup.printings.get(printingId)
    if (!printing) continue
    printings.set(printingId, printing.row)
    const key = lookup.locate({
      table: "printing_image",
      owner: printing.owner,
      bucket: bucketOf([printingId], 64),
      partition: "detail",
    })
    if (key) keys.add(key)
  }
  const wanted = new Set(faces.map((f) => faceKey(f.printingId, f.faceId)))
  const rows = new Map<string, FaceImage>()
  let raw = 0
  for (const key of keys) {
    check()
    raw += integerValue(snapshot.files.get(key)?.["bytes"])
    if (raw > 12 * 1024 * 1024)
      throw new SnapshotError("payload-set", "image page exceeds 12 MiB; reduce visible faces")
    for (const fragment of await client.fragments(key)) {
      if (fragment.table !== "printing_image") continue
      for (const row of fragment.rows) {
        const id = stringValue(row["printing_id"])
        const faceId = stringValue(row["face_id"])
        const key = faceKey(id, faceId)
        if (!wanted.has(key)) continue
        validateMedia(row)
        const printing = printings.get(id)
        const face = faceRows.get(faceId)
        if (
          !printing ||
          !face ||
          face["card_id"] !== printing["card_id"] ||
          !arrayValue(printing["faces"]).some((f) => objectValue(f)["face_id"] === faceId)
        )
          throw new SnapshotError("printing-image-face", "media is not a face of this printing")
        rows.set(key, {
          asset: row,
          source: mediaImageSource(
            client.base,
            integerValue(printing["int_id"]),
            integerValue(face["ordinal"]),
            row,
            "card",
          ),
        })
      }
    }
  }
  check()
  return indexOf(rows)
}

interface FaceImage {
  readonly asset: Row | undefined
  readonly source: ImageSource | undefined
}
const faceCaches = new WeakMap<LoadedSnapshot, Map<string, FaceImage>>()
const MAX_FACES = 64

function indexOf(rows: ReadonlyMap<string, FaceImage>): ImageIndex {
  return {
    known: (printingId, faceId) => rows.has(faceKey(printingId, faceId)),
    eager: (printingId, faceId) => rows.has(faceKey(printingId, faceId)),
    asset: (printingId, faceId) => rows.get(faceKey(printingId, faceId))?.asset,
    cardImage: (printingId, faceId) => rows.get(faceKey(printingId, faceId))?.source,
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
  let cache = faceCaches.get(snapshot)
  if (!cache) {
    cache = new Map()
    faceCaches.set(snapshot, cache)
  }
  const missing = faces.filter((face) => !cache.has(faceKey(face.printingId, face.faceId)))
  if (missing.length) {
    const loaded = await readMediaFaces(client, missing, signal)
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
