import { fail } from "./errors"
import { integerValue, type JsonValue, objectValue, stringValue } from "./json"
import type { Fragment } from "./reader"
import { primaryKey } from "./schema"
import { bucket } from "./sha256"

const CARDS = new Set([
  "card",
  "face",
  "face_revision",
  "card_engine_support",
  "mechanic_projection",
  "card_mechanic_coverage",
  "card_related",
  "digital_link",
  "digital_link_coverage",
  "card_voice",
])
const PRINTINGS = new Set(["printing", "printing_product", "printing_image"])
const ARTS = new Set(["art", "digital_art_link"])

/** Fixed 1.1 membership is checked independently of the producer's file names. */
export function validatePlacement(
  fragments: readonly Fragment[],
  version: string,
  bootstrap: readonly Fragment[] = [],
  knownFaces?: ReadonlyMap<string, JsonValue>,
): void {
  if (version !== "1.1.0" && version !== "2.0.0") return
  const all = [...bootstrap, ...fragments]
  const faces =
    knownFaces ??
    new Map(
      all
        .filter((f) => f.table === "face")
        .flatMap((f) =>
          f.rows.map((row) => [stringValue(row["id"]), row["card_id"] ?? null] as const),
        ),
    )
  for (const fragment of fragments) {
    const owner = objectValue(fragment.value["owner"])
    const kind = stringValue(owner["kind"])
    const home =
      CARDS.has(fragment.table) || PRINTINGS.has(fragment.table) || ARTS.has(fragment.table)
    if (kind !== (home ? "home_set" : "global"))
      fail("owner-mismatch", "incorrect fragment owner kind")
    const id = owner["id"] === null ? "" : stringValue(owner["id"])
    const part = stringValue(fragment.value["partition"])
    const role = ["image_asset", "image_variant", "printing_image"].includes(fragment.table)
      ? "images"
      : part === "bootstrap"
        ? "bootstrap"
        : "text"
    const width =
      role === "images"
        ? kind === "global"
          ? 1
          : version === "2.0.0"
            ? 32
            : 64
        : role === "bootstrap"
          ? kind === "global"
            ? 8
            : ["BP01", "CP04"].includes(id)
              ? 32
              : 64
          : kind === "global" && part === "detail"
            ? 2
            : 32
    const number = integerValue(fragment.value["bucket"])
    // encodeURIComponent leaves these punctuation marks unescaped; the wire recipe does not.
    const encoded = encodeURIComponent(id).replace(
      /[!'()*]/g,
      (c) => `%${c.charCodeAt(0).toString(16).toUpperCase()}`,
    )
    if (
      fragment.file !==
      `${role}/${part}/${kind}/${encoded || "global"}/band/${String(Math.floor(number / width))}`
    )
      fail("fragment-profile", "file does not match fixed band")
    let rows = fragment.rows
    if (fragment.value["base"] !== null) {
      const base = objectValue(fragment.value["base"])
      const file = objectValue(base["file"])
      const found = all.find(
        (f) =>
          f.file === file["key"] &&
          f.table === fragment.table &&
          f.value["partition"] === "bootstrap" &&
          f.value["bucket"] === number,
      )
      if (!found) fail("base-missing", "placement requires bootstrap base")
      rows = found.rows
    }
    for (const row of rows) {
      let values: JsonValue[]
      if (CARDS.has(fragment.table)) {
        const id =
          fragment.table === "card"
            ? row["id"]
            : fragment.table === "face_revision"
              ? faces.get(stringValue(row["face_id"]))
              : fragment.table === "card_related"
                ? row["from_card_id"]
                : row["card_id"]
        if (id === undefined) fail("dangling-reference", "missing card for entity bucket")
        values = [id]
      } else if (PRINTINGS.has(fragment.table))
        values = [row[fragment.table === "printing" ? "id" : "printing_id"] ?? null]
      else if (ARTS.has(fragment.table))
        values = [row[fragment.table === "art" ? "id" : "art_id"] ?? null]
      else if (["image_asset", "image_variant"].includes(fragment.table))
        values = [row[fragment.table === "image_asset" ? "id" : "image_id"] ?? null]
      else values = primaryKey(fragment.table).map((field) => row[field] ?? null)
      if (bucket(values, 64) !== number) fail("fragment-profile", "entity bucket mismatch")
    }
  }
}
