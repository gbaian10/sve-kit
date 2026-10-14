import { fail } from "./format-v3/errors"
import {
  arrayValue,
  canonicalText,
  type JsonObject,
  type JsonValue,
  stringValue,
} from "./format-v3/json"
import type { Files } from "./format-v3/reader"
import { bucket } from "./format-v3/sha256"

interface FragmentRef {
  readonly table: string
  readonly owner: JsonObject
  readonly entityKey: readonly JsonValue[]
  readonly partition: "bootstrap" | "detail" | "history"
  readonly region?: string | null | undefined
  readonly purpose?: string
  readonly segment?: string
}
export type Locator = (ref: FragmentRef) => string | undefined

/** Exact coordinates are indexed without collisions; row lookup derives only fixed physical bands. */
export function createLocator(files: Files, onlyTables?: ReadonlySet<string>): Locator {
  const index = new Map<string, string>()
  const entries: { file: JsonObject; count: JsonValue[] }[] = []
  for (const [key, file] of files)
    for (const raw of arrayValue(file["row_counts"])) {
      const count = arrayValue(raw)
      const table = stringValue(count[0])
      if (onlyTables && !onlyTables.has(table)) continue
      const coordinate = canonicalText([
        table,
        count[1] ?? null,
        count[2] ?? null,
        file["region"] ?? null,
        file["purpose"] ?? null,
        count[4] ?? null,
        count[3] ?? null,
      ])
      if (index.has(coordinate)) fail("shape", "duplicate exact fragment coordinate")
      index.set(coordinate, key)
      entries.push({ file, count })
    }
  return (ref) => {
    if (ref.segment !== undefined && ref.purpose !== undefined && ref.region !== undefined)
      return index.get(
        canonicalText([
          ref.table,
          ref.owner["kind"] ?? null,
          ref.owner["id"] ?? null,
          ref.region,
          ref.purpose,
          ref.segment,
          ref.partition,
        ]),
      )
    const family = ref.owner["id"]
    // Card rows default to the catalog: region-state of the same family also holds card and face.
    const purpose =
      ref.purpose ??
      (ref.partition === "history"
        ? "history"
        : ["text_unit", "translation", "annotation_set"].includes(ref.table)
          ? "dictionary"
          : ["image_asset", "image_variant", "printing_image"].includes(ref.table)
            ? "images"
            : ["card", "face", "face_revision"].includes(ref.table) && ref.partition === "bootstrap"
              ? "catalog"
              : ref.table === "face_revision"
                ? "detail"
                : undefined)
    const width =
      purpose === "images"
        ? family === null
          ? 1
          : 32
        : purpose === "dictionary"
          ? 2
          : family === null
            ? 8
            : ["BP01", "CP04"].includes(stringValue(family))
              ? 32
              : 64
    const band = Math.floor(bucket([...ref.entityKey], 64) / width)
    const matching = entries.filter(
      ({ file, count }) =>
        count[0] === ref.table &&
        count[1] === ref.owner["kind"] &&
        count[2] === family &&
        count[3] === ref.partition &&
        (ref.region === undefined || file["region"] === ref.region || file["region"] === null) &&
        (purpose === undefined || file["purpose"] === purpose) &&
        (ref.segment === undefined || count[4] === ref.segment),
    )
    const scopes = new Set(matching.map(({ file }) => stringValue(file["key"])))
    if (scopes.size === 1) return scopes.values().next().value
    const keys = new Set(
      matching
        .filter(
          ({ file }) =>
            file["purpose"] === "catalog" ||
            stringValue(file["segment"]).endsWith(`:band-${String(band)}`),
        )
        .map(({ file }) => stringValue(file["key"])),
    )
    if (keys.size > 1) fail("shape", "ambiguous row file scope")
    return keys.values().next().value
  }
}
export function bucketOf(primaryKey: readonly JsonValue[], bucketCount: number): number {
  return bucket([...primaryKey], bucketCount)
}
export const GLOBAL_OWNER: JsonObject = { kind: "global", id: null }
export const homeSetOwner = (id: string): JsonObject => ({ kind: "home_set", id })
