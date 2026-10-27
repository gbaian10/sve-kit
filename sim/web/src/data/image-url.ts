import type { Row } from "./format-v3/decode"
import { arrayValue, integerValue, objectValue, stringValue } from "./format-v3/json"
import { imageUrl } from "./image-url-format"
import type { ImageSource } from "./images"

export function mediaImageSource(
  base: string,
  intId: number,
  ordinal: number,
  row: Row,
  purpose: "card" | "art",
): ImageSource | undefined {
  if (row["publication_state"] !== "approved" || row["availability"] !== "available")
    return undefined
  const version = integerValue(row[purpose === "card" ? "card_version" : "art_version"])
  const variants = arrayValue(row["variants"]).map((v) => objectValue(v))
  const find = (key: string) => variants.find((v) => stringValue(v["size_key"]) === key)
  // Each slot names its file explicitly, so one card uses one URL everywhere and downloads once.
  const large = find(purpose === "card" ? "card_l" : "art_s")
  if (!large) return undefined
  const url = (v: Row) => imageUrl(base, intId, ordinal, stringValue(v["size_key"]), version)
  const small = purpose === "card" ? find("card_s") : undefined
  return {
    src: url(large),
    ...(small ? { small: url(small) } : {}),
    width: integerValue(large["width"]),
    height: integerValue(large["height"]),
  }
}
