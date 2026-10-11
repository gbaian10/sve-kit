import { SnapshotError } from "./format-v3/errors"
import { canonicalText, type JsonObject, objectValue, stringValue } from "./format-v3/json"

// An index entry must agree with its manifest on these fields (format §4.1).
const ENTRY_FIELDS = [
  "data_version",
  "published_at",
  "format_version",
  "min_reader_version",
  "required_capabilities",
  "engine_support_target",
] as const

/** The manifest fields an index entry repeats, for stored records that stand in for the manifest. */
export function entryFields(manifest: JsonObject): JsonObject {
  return Object.fromEntries(ENTRY_FIELDS.map((field) => [field, manifest[field] ?? null]))
}

/**
 * The mutable index and the immutable manifest it names come from separate CDN objects; a few
 * small comparisons catch an edge that serves them out of step.
 */
export function checkEntry(entry: JsonObject, manifest: JsonObject): void {
  for (const field of ENTRY_FIELDS)
    if (canonicalText(entry[field] ?? null) !== canonicalText(manifest[field] ?? null))
      throw new SnapshotError("blob-integrity", "index entry differs from its manifest")
}

/** current and previous are two different versions in publication order. */
export function indexEntries(index: JsonObject): {
  current: JsonObject
  previous: JsonObject | null
} {
  const current = objectValue(index["current"])
  const previous = index["previous"] === null ? null : objectValue(index["previous"])
  if (
    previous &&
    (current["data_version"] === previous["data_version"] ||
      stringValue(current["published_at"]) < stringValue(previous["published_at"]))
  )
    throw new SnapshotError("blob-integrity", "invalid current/previous ordering")
  return { current, previous }
}
