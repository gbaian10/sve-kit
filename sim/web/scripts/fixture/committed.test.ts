// @vitest-environment node
import { readFile } from "node:fs/promises"

import { expect, it } from "vitest"

import {
  canonicalText,
  type JsonObject,
  objectValue,
  parseStrict,
  stringValue,
} from "../../src/data/format-v3/json"
import { readSnapshot, readTextAll } from "../../src/data/format-v3/reader"
import { digest } from "../../src/data/format-v3/sha256"
import { CARDS } from "./cards"

const readCommitted = (relative: string) =>
  readFile(new URL(`../../fixtures/snapshot/${relative}`, import.meta.url))

// Read the checked-in bytes independently of the builder, so a stale published fixture fails.
it("reads the committed version index, manifest and payloads with the current reader", async () => {
  const index = objectValue(parseStrict(await readCommitted("snapshots/versions/index.json")))
  expect(index["index_format"]).toBe(2)
  const entry = objectValue(index["current"])
  expect(entry["format_version"]).toBe("3.0.0")
  const manifestBytes = await readCommitted(stringValue(entry["manifest_path"]))
  expect(digest(manifestBytes)).toBe(entry["manifest_sha256"])
  const manifest = objectValue(parseStrict(manifestBytes))
  expect(manifest["data_version"]).toBe(entry["data_version"])
  const payloads = new Map<string, Uint8Array>()
  for (const file of manifest["files"] as JsonObject[]) {
    const bytes = await readCommitted(stringValue(file["path"]))
    expect(digest(bytes)).toBe(file["sha256"])
    expect(bytes.length).toBe(file["bytes"])
    payloads.set(stringValue(file["key"]), bytes)
  }
  const view = readSnapshot(manifest, payloads)
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
  const textAll = objectValue(manifest["text_all"])
  const bytes = await readCommitted(stringValue(textAll["path"]))
  expect(digest(bytes)).toBe(textAll["sha256"])
  expect(bytes.length).toBe(textAll["bytes"])
  const attachments = new Map(
    (manifest["files"] as JsonObject[])
      .filter((file) => ["images", "programs"].includes(stringValue(file["role"])))
      .map((file) => [
        stringValue(file["key"]),
        payloads.get(stringValue(file["key"])) ?? new Uint8Array(),
      ]),
  )
  expect(canonicalText(readTextAll(manifest, bytes, attachments))).toBe(canonicalText(view))
})
