import { fail } from "./errors"
import { integerValue, type JsonObject, type JsonValue, objectValue, stringValue } from "./json"
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

/** Fixed 3.0 membership is checked independently of the producer's file names. */
export function validatePlacement(
  fragments: readonly Fragment[],
  version: string,
  bootstrap: readonly Fragment[] = [],
  knownFaces?: ReadonlyMap<string, JsonValue>,
): void {
  placementChecker(version, bootstrap, knownFaces)(fragments)
}

type FaceCards = ReadonlyMap<string, JsonValue>

function faceCards(fragments: readonly Fragment[]): Map<string, JsonValue> {
  return new Map(
    fragments
      .filter((f) => f.table === "face")
      .flatMap((f) =>
        f.rows.map((row) => [stringValue(row["id"]), row["card_id"] ?? null] as const),
      ),
  )
}

function originalRevisions(fragments: readonly Fragment[]): Map<string, JsonObject> {
  return new Map(
    fragments
      .filter((f) => f.table === "face_revision" && f.value["base"] === null)
      .flatMap((f) => f.rows.map((row) => [stringValue(row["id"]), row] as const)),
  )
}

/**
 * The checks of `validatePlacement` with the bootstrap indexed once. Startup validates every
 * bootstrap fragment on its own; rebuilding these indexes for each one made loading quadratic.
 * Rows of the checked fragments take precedence over bootstrap rows with the same id.
 */
export function placementChecker(
  version: string,
  bootstrap: readonly Fragment[] = [],
  knownFaces?: FaceCards,
): (fragments: readonly Fragment[]) => void {
  if (version !== "3.0.0") fail("unsupported-version", "unsupported placement profile")
  let bootstrapFaces: FaceCards | undefined
  let bootstrapRevisions: ReadonlyMap<string, JsonObject> | undefined
  let bases: ReadonlyMap<string, Fragment> | undefined
  const baseKey = (file: JsonValue | undefined, table: string, bucket: JsonValue | undefined) =>
    JSON.stringify([file ?? null, table, bucket ?? null])
  return (fragments) => {
    const ownFaces = faceCards(fragments)
    const needsFaces = fragments.some(
      (f) => f.table === "face_revision" || f.table === "field_annotation",
    )
    const cardOfFace = (id: string): JsonValue | undefined => {
      if (knownFaces) return knownFaces.get(id)
      if (!needsFaces) return undefined
      if (ownFaces.has(id)) return ownFaces.get(id)
      bootstrapFaces ??= faceCards(bootstrap)
      return bootstrapFaces.get(id)
    }
    let ownRevisions: ReadonlyMap<string, JsonObject> | undefined
    const revisionOf = (id: string): JsonObject | undefined => {
      ownRevisions ??= originalRevisions(fragments)
      if (ownRevisions.has(id)) return ownRevisions.get(id)
      bootstrapRevisions ??= originalRevisions(bootstrap)
      return bootstrapRevisions.get(id)
    }
    const baseOf = (file: JsonValue | undefined, table: string, bucket: JsonValue | undefined) => {
      if (!bases) {
        const index = new Map<string, Fragment>()
        for (const f of bootstrap)
          if (f.value["partition"] === "bootstrap") {
            const key = baseKey(f.file, f.table, f.value["bucket"])
            if (!index.has(key)) index.set(key, f)
          }
        bases = index
      }
      return (
        bases.get(baseKey(file, table, bucket)) ??
        fragments.find(
          (f) =>
            f.file === file &&
            f.table === table &&
            f.value["partition"] === "bootstrap" &&
            f.value["bucket"] === bucket,
        )
      )
    }
    checkFragments(fragments, cardOfFace, revisionOf, baseOf)
  }
}

function checkFragments(
  fragments: readonly Fragment[],
  cardOfFace: (id: string) => JsonValue | undefined,
  revisionOf: (id: string) => JsonObject | undefined,
  baseOf: (
    file: JsonValue | undefined,
    table: string,
    bucket: JsonValue | undefined,
  ) => Fragment | undefined,
): void {
  for (const fragment of fragments) {
    const owner = objectValue(fragment.value["owner"])
    const kind = stringValue(owner["kind"])
    const home =
      CARDS.has(fragment.table) ||
      PRINTINGS.has(fragment.table) ||
      ARTS.has(fragment.table) ||
      (fragment.table === "field_annotation" &&
        fragment.rows.some((row) =>
          ["face_revision", "printing_face"].includes(
            stringValue(objectValue(row["owner"])["kind"]),
          ),
        ))
    if (
      fragment.table === "field_annotation" && fragment.rows.length === 0
        ? !["home_set", "global"].includes(kind)
        : kind !== (home ? "home_set" : "global")
    )
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
          : 32
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
      const found = baseOf(file["key"], fragment.table, number)
      if (!found) fail("base-missing", "placement requires bootstrap base")
      rows = found.rows
    }
    for (const row of rows) {
      let values: JsonValue[]
      if (fragment.table === "field_annotation") {
        const source = objectValue(row["owner"])
        if (source["kind"] === "face_revision") {
          const revision = revisionOf(stringValue(source["id"]))
          const card = revision && cardOfFace(stringValue(revision["face_id"]))
          if (card === undefined)
            fail("public-annotation/owner", "missing original revision for annotation bucket")
          values = [card]
        } else if (source["kind"] === "printing_face") values = [source["id"] ?? null]
        else values = [source, row["field"] ?? null, row["ordinal"] ?? null]
      } else if (CARDS.has(fragment.table)) {
        const id =
          fragment.table === "card"
            ? row["id"]
            : fragment.table === "face_revision"
              ? cardOfFace(stringValue(row["face_id"]))
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
