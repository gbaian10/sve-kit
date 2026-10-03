import {
  canonical,
  type JsonObject,
  type JsonValue,
  objectValue,
  stringValue,
} from "./format-v1/json"
import { digest } from "./format-v1/sha256"

const golden = import.meta.glob<string>(
  "../../../../tests/fixtures/snapshot-contract/v2/**/*.json",
  { query: "?raw", import: "default", eager: true },
)
export function v2Fixture(name: string): JsonValue {
  const key = Object.keys(golden).find((k) => k.endsWith(`/v2/${name}`))
  if (!key) throw new Error("missing shared v2 fixture")
  return JSON.parse(golden[key] ?? "") as JsonValue
}

/** A mutable fake origin per test; the golden and oracle remain independently written. */
export function v2Version(
  number = 1,
  mutate?: (table: string, row: JsonValue[]) => void,
  mutateManifest?: (manifest: JsonObject) => void,
) {
  const manifest = objectValue(v2Fixture("manifest.json"))
  manifest["data_version"] = `20261004T01000${String(number)}Z-0001`
  manifest["published_at"] = `2026-10-04T01:00:0${String(number)}Z`
  const files = new Map<string, Uint8Array>()
  for (const file of manifest["files"] as JsonObject[]) {
    const value = objectValue(v2Fixture(`payloads/${stringValue(file["sha256"]).slice(7)}.json`))
    if (mutate && value["tables"]) {
      for (const [table, fragments] of Object.entries(objectValue(value["tables"])))
        for (const fragment of fragments as JsonObject[])
          for (const row of fragment["rows"] as JsonValue[][]) mutate(table, row)
    }
    const bytes = canonical(value)
    const old = file["sha256"]
    file["sha256"] = digest(bytes)
    file["bytes"] = bytes.length
    file["path"] = `snapshots/blobs/${digest(bytes).slice(7)}.json`
    if (old !== file["sha256"]) {
      for (const dependent of manifest["files"] as JsonObject[])
        for (const dep of dependent["dependencies"] as JsonObject[])
          if (dep["key"] === file["key"]) dep["sha256"] = file["sha256"] ?? null
    }
    files.set(stringValue(file["path"]), bytes)
  }
  mutateManifest?.(manifest)
  const bytes = canonical(manifest)
  const path = `snapshots/manifests/${digest(bytes).slice(7)}.json`
  files.set(path, bytes)
  const entry: JsonObject = { manifest_path: path, manifest_sha256: digest(bytes) }
  for (const field of [
    "data_version",
    "published_at",
    "format_version",
    "min_reader_version",
    "required_capabilities",
    "engine_support_target",
  ])
    entry[field] = manifest[field] ?? null
  return { manifest, files, entry }
}
