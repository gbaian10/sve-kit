import type { SnapshotClient } from "./client"
import type { Row } from "./format-v1/decode"
import { integerValue, stringValue } from "./format-v1/json"

export interface ImageSource {
  readonly src: string
  readonly srcSet: string
  readonly width: number
  readonly height: number
}

export interface ImageIndex {
  /** The card image of one printing face, or undefined when withdrawn, pending or missing. */
  readonly cardImage: (printingId: string, faceId: string) => ImageSource | undefined
  readonly asset: (printingId: string, faceId: string) => Row | undefined
}

const CARD_SIZES = ["card_s", "card_m", "card_l"]

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

/** Loads the images file once and indexes printing faces to their assets and variants. */
export async function loadImageIndex(client: SnapshotClient): Promise<ImageIndex> {
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("snapshot not loaded")
  const keys = [...snapshot.files]
    .filter(([, file]) => file["role"] === "images")
    .map(([key]) => key)
  const fragments = (await Promise.all(keys.map((key) => client.fragments(key)))).flat()
  const assets = new Map<string, Row>()
  const variants = new Map<string, Row[]>()
  const faceImage = new Map<string, string>()
  for (const fragment of fragments) {
    for (const row of fragment.rows) {
      if (fragment.table === "image_asset") assets.set(stringValue(row["id"]), row)
      else if (fragment.table === "image_variant") {
        const id = stringValue(row["image_id"])
        variants.set(id, [...(variants.get(id) ?? []), row])
      } else if (fragment.table === "printing_image") {
        faceImage.set(
          `${stringValue(row["printing_id"])}\u0000${stringValue(row["face_id"])}`,
          stringValue(row["image_id"]),
        )
      }
    }
  }
  const imageId = (printingId: string, faceId: string) =>
    faceImage.get(`${printingId}\u0000${faceId}`)
  return {
    asset: (printingId, faceId) => {
      const id = imageId(printingId, faceId)
      return id === undefined ? undefined : assets.get(id)
    },
    cardImage: (printingId, faceId) => {
      const id = imageId(printingId, faceId)
      return id === undefined ? undefined : imageSource(client.base, variants.get(id) ?? [])
    },
  }
}
