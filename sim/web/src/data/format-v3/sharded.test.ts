// @vitest-environment node
import { describe, expect, it } from "vitest"

import {
  canonical,
  canonicalText,
  type JsonObject,
  type JsonValue,
  objectValue,
  stringValue,
} from "./json"
import { readContainer, readSnapshot, readTextAll, verifyManifest } from "./reader"
import { digest } from "./sha256"

const files = import.meta.glob<string>(
  "../../../../../tests/fixtures/snapshot-contract/v3/**/*.json",
  { query: "?raw", import: "default", eager: true },
)
function fixture(path: string): JsonValue {
  const key = Object.keys(files).find((key) => key.endsWith(`/v3/${path}`))
  if (!key) throw new Error("missing shared fixture")
  return JSON.parse(files[key] ?? "") as JsonValue
}
const manifest = objectValue(fixture("manifest.json"))
const payloads = new Map(
  (manifest["files"] as JsonObject[]).map((file) => [
    stringValue(file["key"]),
    canonical(fixture(`payloads/${stringValue(file["sha256"]).slice(7)}.json`)),
  ]),
)

describe("2.0 shared wire contract", () => {
  it("joins the independently written multibucket golden to the unchanged logical view", () => {
    expect(canonicalText(readSnapshot(manifest, payloads))).toBe(
      canonicalText(fixture("expected-logical.json")),
    )
    const attachments = new Map(
      [...payloads].filter(([key]) => key.startsWith("images/") || key === "programs"),
    )
    expect(
      canonicalText(readTextAll(manifest, canonical(fixture("text-all.json")), attachments)),
    ).toBe(canonicalText(fixture("expected-logical.json")))
  })
  it("requires the exact profile and capabilities and rejects a mixed-minor payload", () => {
    const value = structuredClone(manifest)
    value["required_capabilities"] = ["column-partition-v1", "fragment-container-v1"]
    expect(() => verifyManifest(value)).toThrow(/schema|unsupported-version/)
    const file = (manifest["files"] as JsonObject[]).find((file) => file["role"] === "images")
    if (!file) throw new Error("missing image fixture")
    const container = objectValue(fixture(`payloads/${stringValue(file["sha256"]).slice(7)}.json`))
    container["format_version"] = "1.0.0"
    expect(() => readContainer(file, container, "3.0.0")).toThrow(/schema/)
  })
  it("rejects data files over the frozen raw budget before requesting payloads", () => {
    const value = structuredClone(manifest)
    const file = (value["files"] as JsonObject[])[0]
    if (!file) throw new Error("missing fixture file")
    file["bytes"] = 512 * 1024 + 1
    expect(() => verifyManifest(value)).toThrow(/fragment-profile/)
  })
  it("rejects duplicate manifest fragment identities before payload fetching", () => {
    const value = structuredClone(manifest)
    const file = (value["files"] as JsonObject[]).find(
      (f) => (f["row_counts"] as JsonObject[]).length > 0,
    )
    if (!file) throw new Error("missing fixture")
    const counts = file["row_counts"] as JsonObject[]
    const first = counts[0]
    if (!first) throw new Error("missing count")
    counts.push(structuredClone(first))
    expect(() => verifyManifest(value)).toThrow("duplicate-fragment")
  })
  it("rejects a container bucket outside N even when the declared row counts agree", () => {
    const file = structuredClone(
      (manifest["files"] as JsonObject[]).find((f) => f["role"] === "images"),
    )
    if (!file) throw new Error("missing fixture")
    const container = objectValue(fixture(`payloads/${stringValue(file["sha256"]).slice(7)}.json`))
    for (const fragments of Object.values(objectValue(container["tables"])))
      for (const entry of fragments as JsonObject[]) entry["bucket"] = 64
    for (const count of file["row_counts"] as JsonObject[]) count["bucket"] = 64
    expect(() => readContainer(file, container, "3.0.0")).toThrow("fragment-profile")
  })
  it("rejects a resealed variant bucket that follows its full PK instead of image id", () => {
    const m = structuredClone(manifest)
    const values = new Map(payloads)
    const file = (m["files"] as JsonObject[]).find((file) =>
      (file["row_counts"] as JsonObject[]).some((count) => count["table"] === "image_variant"),
    )
    if (!file) throw new Error("missing variant fixture")
    const container = objectValue(
      JSON.parse(new TextDecoder().decode(values.get(stringValue(file["key"])))) as JsonValue,
    )
    const fragments = objectValue(container["tables"])["image_variant"] as JsonObject[]
    const fragment = fragments[0]
    if (!fragment) throw new Error("missing fragment")
    fragment["bucket"] = (Number(fragment["bucket"]) + 1) % 64
    const data = canonical(container)
    file["sha256"] = digest(data)
    file["bytes"] = data.length
    file["path"] = `snapshots/blobs/${digest(data).slice(7)}.json`
    for (const count of file["row_counts"] as JsonObject[])
      if (count["table"] === "image_variant") count["bucket"] = fragment["bucket"] ?? null
    values.set(stringValue(file["key"]), data)
    expect(() => readSnapshot(m, values)).toThrow(/fragment-profile/)
  })
})
