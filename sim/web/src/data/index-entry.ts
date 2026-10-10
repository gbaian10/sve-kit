import { SnapshotError } from "./format-v3/errors"
import {
  arrayValue,
  canonicalText,
  type JsonObject,
  objectValue,
  stringValue,
} from "./format-v3/json"
import { validate } from "./format-v3/schema"

export function validateIndex2(index: JsonObject): void {
  validate("Index", index, [], "3.0.0")
  const entries = [
    objectValue(index["current"]),
    ...(index["previous"] === null ? [] : [objectValue(index["previous"])]),
  ]
  for (const entry of entries) {
    const capabilities = arrayValue(entry["required_capabilities"]).map((value) =>
      stringValue(value),
    )
    if (
      new Set(capabilities).size !== capabilities.length ||
      canonicalText([...capabilities].sort()) !== canonicalText(capabilities)
    )
      throw new SnapshotError("schema", "Index capabilities must be sorted and unique")
    const hash = stringValue(entry["manifest_sha256"]).slice(7)
    if (entry["manifest_path"] !== `snapshots/manifests/${hash}.json`)
      throw new SnapshotError("blob-integrity", "Index manifest path differs from hash")
  }
}
