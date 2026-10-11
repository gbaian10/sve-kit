// @vitest-environment node
import { readFile } from "node:fs/promises"

import { expect, it } from "vitest"

import { type JsonObject, objectValue, parseJson, stringValue } from "../../src/data/format-v3/json"
import { digest } from "../../src/data/format-v3/sha256"
import { CARDS } from "./cards"
import { snapshotView } from "./view"

const readCommitted = (relative: string) =>
  readFile(new URL(`../../fixtures/snapshot/${relative}`, import.meta.url))

// Read the checked-in bytes independently of the builder, so a stale published fixture fails.
it("reads the committed version index, manifest and payloads with the current reader", async () => {
  const index = objectValue(parseJson(await readCommitted("snapshots/versions/index.json")))
  expect(index["index_format"]).toBe(2)
  const entry = objectValue(index["current"])
  expect(entry["format_version"]).toBe("3.0.0")
  const manifestBytes = await readCommitted(stringValue(entry["manifest_path"]))
  expect(digest(manifestBytes)).toBe(entry["manifest_sha256"])
  const manifest = objectValue(parseJson(manifestBytes))
  expect(manifest["data_version"]).toBe(entry["data_version"])
  const payloads = new Map<string, Uint8Array>()
  for (const file of manifest["files"] as JsonObject[]) {
    const bytes = await readCommitted(stringValue(file["path"]))
    expect(digest(bytes)).toBe(file["sha256"])
    expect(bytes.length).toBe(file["bytes"])
    payloads.set(stringValue(file["key"]), bytes)
  }
  const view = snapshotView(manifest, payloads)
  expect(view["card"]?.map((card) => card["id"]).sort()).toEqual(
    CARDS.map((card) => card.id).sort(),
  )
  expect(view["printing"]).toHaveLength(CARDS.reduce((n, card) => n + card.printings.length, 0))
  const printing = view["printing"]?.find((row) => row["id"] === "p:bp01-020")
  const face = (printing?.["faces"] as JsonObject[])[0]
  expect(face?.["observations"]).toEqual([
    {
      revision_id: "r:f:bp01-020:jp:1",
      state: "available",
      source_url: "https://example.invalid/synthetic-observation",
    },
  ])
})
