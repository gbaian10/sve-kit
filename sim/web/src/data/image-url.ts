import type { Row } from "./format-v1/decode"
import { SnapshotError } from "./format-v1/errors"
import { arrayValue, integerValue, objectValue, stringValue } from "./format-v1/json"
import { validateMedia } from "./format-v1/media"
import type { ImageSource } from "./images"

const SIZES = new Set(["card_s", "card_m", "card_l", "art_s", "art_m"])

/** Permanent IDs are decimal values, never parsed from card numbers or array positions. */
export function imageUrl(
  base: string,
  intId: number,
  ordinal: number,
  size: string,
  version: number,
): string {
  if (
    !Number.isSafeInteger(intId) ||
    intId < 1 ||
    !Number.isSafeInteger(ordinal) ||
    ordinal < 0 ||
    !Number.isSafeInteger(version) ||
    version < 1 ||
    !SIZES.has(size)
  )
    throw new SnapshotError("image-variant-unapproved", "invalid image URL components")
  return `${base}/images/${size}/${String(intId)}${ordinal === 0 ? "" : `-f${String(ordinal)}`}.webp?v=${String(version)}`
}

export function mediaImageSource(
  base: string,
  intId: number,
  ordinal: number,
  row: Row,
  purpose: "card" | "art",
): ImageSource | undefined {
  validateMedia(row)
  if (row["publication_state"] !== "approved" || row["availability"] !== "available")
    return undefined
  const version = integerValue(row[purpose === "card" ? "card_version" : "art_version"])
  const variants = arrayValue(row["variants"])
    .map((v) => objectValue(v))
    .filter((v) => stringValue(v["size_key"]).startsWith(`${purpose}_`))
    .sort((a, b) => integerValue(a["width"]) - integerValue(b["width"]))
  const widths = new Map<number, Row>()
  // Non-upscaled tiny images can have identical widths; duplicate width descriptors are invalid.
  for (const variant of variants)
    if (!widths.has(integerValue(variant["width"])))
      widths.set(integerValue(variant["width"]), variant)
  const distinct = [...widths.values()]
  const largest = distinct.at(-1)
  if (!largest) return undefined
  const url = (v: Row) => imageUrl(base, intId, ordinal, stringValue(v["size_key"]), version)
  return {
    src: url(largest),
    srcSet: distinct.map((v) => `${url(v)} ${String(integerValue(v["width"]))}w`).join(", "),
    width: integerValue(largest["width"]),
    height: integerValue(largest["height"]),
  }
}
