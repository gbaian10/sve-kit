import {
  arrayValue,
  canonical,
  type JsonObject,
  type JsonValue,
  objectValue,
  stringValue,
} from "./format-v3/json"
import { digest } from "./format-v3/sha256"

const golden = import.meta.glob<string>(
  "../../../../tests/fixtures/snapshot-contract/v3/**/*.json",
  { query: "?raw", import: "default", eager: true },
)
export function v3Fixture(name: string): JsonValue {
  const key = Object.keys(golden).find((k) => k.endsWith(`/v3/${name}`))
  if (!key) throw new Error("missing shared v3 fixture")
  return JSON.parse(golden[key] ?? "") as JsonValue
}

/** A mutable fake origin per test; the golden and oracle remain independently written. */
export function v3Version(
  number = 1,
  mutate?: (table: string, row: JsonValue[]) => void,
  mutateManifest?: (manifest: JsonObject) => void,
) {
  const manifest = objectValue(v3Fixture("manifest.json"))
  manifest["data_version"] = `20261004T01000${String(number)}Z-0001`
  manifest["published_at"] = `2026-10-04T01:00:0${String(number)}Z`
  const files = new Map<string, Uint8Array>()
  const metadata = manifest["files"] as JsonObject[]
  const oldHashes = new Map(metadata.map((f) => [stringValue(f["key"]), f["sha256"]]))
  const hashes = new Map(oldHashes)
  const rebind = (value: JsonValue): void => {
    if (Array.isArray(value)) {
      for (const item of value) rebind(item)
      return
    }
    if (value === null || typeof value !== "object") return
    if (
      Object.keys(value).length === 2 &&
      typeof value["key"] === "string" &&
      value["sha256"] === oldHashes.get(value["key"])
    )
      value["sha256"] = hashes.get(value["key"]) ?? null
    else for (const item of Object.values(value)) rebind(item)
  }
  const priority = (file: JsonObject) =>
    file["purpose"] === "config" || file["purpose"] === "programs"
      ? 0
      : file["purpose"] === "catalog"
        ? 1
        : file["purpose"] === "print" || file["purpose"] === "region-state"
          ? 2
          : file["purpose"] === "images"
            ? 4
            : 3
  for (const file of [...metadata].sort(
    (a, b) =>
      priority(a) - priority(b) || (a["region"] === null ? 0 : 1) - (b["region"] === null ? 0 : 1),
  )) {
    const value = objectValue(v3Fixture(`payloads/${stringValue(file["sha256"]).slice(7)}.json`))
    if (mutate && value["tables"]) {
      for (const [table, fragments] of Object.entries(objectValue(value["tables"])))
        for (const raw of arrayValue(fragments))
          for (const row of arrayValue(arrayValue(raw)[4])) mutate(table, arrayValue(row))
    }
    rebind(value)
    const bytes = canonical(value)
    file["sha256"] = digest(bytes)
    file["bytes"] = bytes.length
    file["path"] = `snapshots/blobs/${digest(bytes).slice(7)}.json`
    hashes.set(stringValue(file["key"]), file["sha256"])
    files.set(stringValue(file["path"]), bytes)
  }
  for (const file of metadata) rebind(file["dependencies"] ?? [])
  rebind(manifest["config_ref"] ?? null)
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
