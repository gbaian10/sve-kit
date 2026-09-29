import {
  canonicalText,
  type JsonObject,
  type JsonValue,
  objectValue,
  stringValue,
} from "./format-v1/json"
import type { Files } from "./format-v1/reader"
import { bucket } from "./format-v1/sha256"

interface FragmentRef {
  readonly table: string
  readonly owner: JsonObject
  readonly bucket: number
  readonly partition: "bootstrap" | "detail" | "history"
}

export type Locator = (ref: FragmentRef) => string | undefined

const identity = (ref: FragmentRef): string =>
  canonicalText([ref.table, ref.owner, ref.bucket, ref.partition])

/**
 * Fragment identity is unique per manifest (transport §4), so `files[].row_counts` is an exact
 * table from (table, owner, bucket, partition) to the one file that holds it.
 */
export function createLocator(files: Files): Locator {
  const index = new Map<string, string>()
  for (const [key, file] of files) {
    for (const count of file["row_counts"] as JsonValue[]) {
      const entry = objectValue(count)
      index.set(
        identity({
          table: stringValue(entry["table"]),
          owner: objectValue(entry["owner"]),
          bucket: Number(entry["bucket"]),
          partition: entry["partition"] as FragmentRef["partition"],
        }),
        key,
      )
    }
  }
  return (ref) => index.get(identity(ref))
}

/** sha256-mod-v1 over the canonical primary-key array (transport §5). */
export function bucketOf(primaryKey: readonly JsonValue[], bucketCount: number): number {
  return bucket([...primaryKey], bucketCount)
}

export const GLOBAL_OWNER: JsonObject = { kind: "global", id: null }
export const homeSetOwner = (id: string): JsonObject => ({ kind: "home_set", id })
