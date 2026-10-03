import type { SnapshotClient } from "./client"
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
