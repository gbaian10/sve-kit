import { decodeRow, type Row } from "./decode"
import { fail } from "./errors"
import {
  arrayValue,
  canonicalText,
  compareCodePoints,
  integerValue,
  type JsonObject,
  type JsonValue,
  objectValue,
  stringValue,
} from "./json"
import { rowType } from "./schema"

const FORMAT = "3.0.0"
/** This reader's own contract version (transport §1.1), independent of the web app version. */
const READER_CONTRACT_VERSION = "3.0.0"
const CAPABILITIES: readonly string[] = [
  "column-partition-v1",
  "fragment-container-v1",
  "image-entity-buckets-v1",
  "rules-name-on-demand-v1",
  "digital-same-name-links-v1",
  "image-id-url-v1",
  "jp-source-translation-v1",
  "public-annotation-v1",
]

export type View = Record<string, Row[]>

export interface Fragment {
  readonly file: string
  readonly table: string
  readonly value: JsonObject
  readonly rows: Row[]
  /** canonical [table, owner, bucket, partition] */
  readonly identity: string
}

export type Files = Map<string, JsonObject>

function versionTuple(version: string): number[] {
  return version.split(".").map((part) => Number(part))
}

function newerThan(version: string, reference: readonly number[]): boolean {
  const parts = versionTuple(version)
  for (let i = 0; i < reference.length; i += 1) {
    const a = parts[i] ?? 0
    const b = reference[i] ?? 0
    if (a !== b) return a > b
  }
  return false
}

/** Whether a manifest or version-index entry can be read by this reader (transport §1.1). */
export function isCompatible(entry: JsonObject): boolean {
  const capabilities = arrayValue(entry["required_capabilities"] ?? null).map((item) =>
    stringValue(item),
  )
  return (
    stringValue(entry["format_version"]) === FORMAT &&
    !newerThan(stringValue(entry["min_reader_version"]), versionTuple(READER_CONTRACT_VERSION)) &&
    capabilities.length === CAPABILITIES.length &&
    CAPABILITIES.every((capability) => capabilities.includes(capability)) &&
    capabilities.every((capability) => CAPABILITIES.includes(capability))
  )
}

/** Decode an already hash-verified published manifest. */
export function readManifest(value: JsonValue): { manifest: JsonObject; files: Files } {
  const manifest = objectValue(value)
  if (!isCompatible(manifest))
    fail("unsupported-version", "unsupported format, reader version or capability")
  const files = new Map(
    arrayValue(manifest["files"]).map((item) => {
      const file = objectValue(item)
      return [stringValue(file["key"]), file] as const
    }),
  )
  return { manifest, files }
}

export function readContainer(file: JsonObject, value: JsonObject, version = "3.0.0"): Fragment[] {
  const key = stringValue(file["key"])
  return Object.entries(objectValue(value["tables"])).flatMap(([table, entries]) =>
    arrayValue(entries).map((entry) => {
      const fragment = objectValue(entry)
      const partition = stringValue(fragment["partition"])
      const rows = arrayValue(fragment["rows"]).map((row) =>
        decodeRow(rowType(table, partition, version), row, [], version),
      )
      const { rows: _rows, ...metadata } = fragment
      return {
        file: key,
        table,
        value: metadata,
        rows,
        identity: canonicalText([
          table,
          fragment["owner"] ?? null,
          fragment["bucket"] ?? null,
          partition,
        ]),
      }
    }),
  )
}

export function findBase(detail: Fragment, fragments: readonly Fragment[]): Fragment {
  const ref = objectValue(detail.value["base"])
  const key = stringValue(objectValue(ref["file"])["key"])
  const found = fragments.find(
    (fragment) =>
      fragment.file === key &&
      fragment.table === detail.table &&
      fragment.value["partition"] === "bootstrap" &&
      fragment.value["bucket"] === ref["bucket"] &&
      canonicalText(fragment.value["owner"] ?? null) === canonicalText(ref["owner"] ?? null),
  )
  if (!found) fail("base-missing", "detail bootstrap is missing")
  return found
}

export function joinDetail(detail: Fragment, base: Fragment, faces: Map<string, Row>): Row[] {
  return detail.rows.map((row) => {
    const first = base.rows[integerValue(row["row_index"])]
    if (!first) fail("row-index", "detail bootstrap row is missing")
    const { row_index: _index, ...rest } = row
    if (detail.table === "printing") {
      const details = new Map(
        arrayValue(row["faces"]).map((raw) => {
          const face = objectValue(raw)
          const { face_ordinal: _ordinal, ...fields } = face
          return [integerValue(face["face_ordinal"]), fields] as const
        }),
      )
      return {
        ...first,
        faces: arrayValue(first["faces"]).map((raw) => {
          const face = objectValue(raw)
          const fields = details.get(
            integerValue(faces.get(stringValue(face["face_id"]))?.["ordinal"]),
          )
          if (!fields) fail("face-ordinal", "printing face detail is missing")
          return { ...face, ...fields }
        }),
      }
    }
    const translations = [...arrayValue(first["translations"]), ...arrayValue(rest["translations"])]
    translations.sort((a, b) => {
      const left = objectValue(a),
        right = objectValue(b)
      return (
        compareCodePoints(stringValue(left["field"]), stringValue(right["field"])) ||
        (left["ordinal"] === null ? -1 : integerValue(left["ordinal"])) -
          (right["ordinal"] === null ? -1 : integerValue(right["ordinal"])) ||
        compareCodePoints(stringValue(left["target_lang"]), stringValue(right["target_lang"]))
      )
    })
    return { ...first, ...rest, translations }
  })
}
