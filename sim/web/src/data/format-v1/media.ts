import type { Row } from "./decode"
import { fail } from "./errors"
import { arrayValue, canonicalText, integerValue, objectValue, stringValue } from "./json"
import type { View } from "./reader"

const SIZES = ["art_m", "art_s", "card_l", "card_m", "card_s"]

/** Display metadata is sufficient for first paint; source details never gate a valid media row. */
export function validateMedia(row: Row): void {
  const ready = row["publication_state"] === "approved" && row["availability"] === "available"
  const variants = arrayValue(row["variants"])
  if (!ready) {
    if (row["card_version"] !== null || row["art_version"] !== null || variants.length !== 0)
      fail("image-variant-unapproved", "unpublished media cannot expose versions or sizes")
    return
  }
  for (const field of ["card_version", "art_version"])
    if (integerValue(row[field]) <= 0) fail("image-variant-unapproved", "media requires a version")
  const sizes = variants.map((v) => stringValue(objectValue(v)["size_key"]))
  if (sizes.join("\n") !== SIZES.join("\n"))
    fail("image-variant-unapproved", "available media requires five sorted unique sizes")
  for (const raw of variants) {
    const variant = objectValue(raw)
    if (integerValue(variant["width"]) <= 0 || integerValue(variant["height"]) <= 0)
      fail("image-variant-unapproved", "media dimensions must be positive")
  }
}

/** Only the complete logical reader has both display projection and optional source details. */
export function validateMediaDetails(view: View): void {
  const assets = new Map((view["image_asset"] ?? []).map((r) => [stringValue(r["id"]), r]))
  const variants = new Map(
    (view["image_variant"] ?? []).map((r) => [
      `${stringValue(r["image_id"])}\0${stringValue(r["size_key"])}`,
      r,
    ]),
  )
  for (const row of view["printing_image"] ?? []) {
    validateMedia(row)
    const asset = assets.get(stringValue(row["image_id"]))
    for (const field of ["publication_state", "availability", "withdrawal_reason"])
      if (!asset || canonicalText(asset[field] ?? null) !== canonicalText(row[field] ?? null))
        fail("image-variant-unapproved", "media state differs from source details")
    for (const raw of arrayValue(row["variants"])) {
      const variant = objectValue(raw)
      const detail = variants.get(
        `${stringValue(row["image_id"])}\0${stringValue(variant["size_key"])}`,
      )
      if (!detail || detail["width"] !== variant["width"] || detail["height"] !== variant["height"])
        fail("image-variant-unapproved", "media size differs from source details")
    }
  }
}
