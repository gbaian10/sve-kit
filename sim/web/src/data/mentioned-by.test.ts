// @vitest-environment node
import { describe, expect, it } from "vitest"

import { memoryCache } from "../test-utils/cache"
import { createSnapshotClient } from "./client"
import { arrayValue, canonical, type JsonObject, objectValue, stringValue } from "./format-v3/json"
import { isCompatible, readContainer, readManifest } from "./format-v3/reader"
import { digest } from "./format-v3/sha256"
import { createLocator, homeSetOwner } from "./locator"
import { v3Fixture } from "./v3-fixture"

describe("current card-name back references", () => {
  it("locates and decodes the target card's engine band through the snapshot client", async () => {
    const native = objectValue(v3Fixture("mentioned-by-native.json"))
    const manifest = objectValue(native["manifest"])
    const hash = digest(canonical(manifest))
    const entry: JsonObject = {
      ...manifest,
      manifest_path: `snapshots/manifests/${hash.slice(7)}.json`,
      manifest_sha256: hash,
    }
    const files = new Map<string, Uint8Array>([
      [stringValue(entry["manifest_path"]), canonical(manifest)],
    ])
    for (const file of manifest["files"] as JsonObject[])
      files.set(
        stringValue(file["path"]),
        canonical(objectValue(native["payloads"])[stringValue(file["key"])] ?? null),
      )
    const requests: string[] = []
    const client = createSnapshotClient("https://cdn.test", {
      cacheStorage: memoryCache(),
      fetch: (url) => {
        const path = url.slice("https://cdn.test/".length)
        requests.push(path)
        const bytes =
          path === "snapshots/versions/index.json"
            ? canonical({ index_format: 2, revision: 1, current: entry, previous: null })
            : files.get(path)
        return Promise.resolve(
          bytes ? new Response(bytes.slice().buffer) : new Response(null, { status: 404 }),
        )
      },
    })
    await client.load()
    const snapshot = client.snapshot()
    if (!snapshot) throw new Error("missing synthetic snapshot")
    const file = createLocator(snapshot.files)({
      table: "card_mentioned_by",
      owner: homeSetOwner("family"),
      entityKey: ["old_card"],
      partition: "detail",
      region: "jp",
      purpose: "engine",
    })
    expect(file).toBe("engine/shared/family-family/0")
    const related = createLocator(snapshot.files)({
      table: "card_related",
      owner: homeSetOwner("family"),
      entityKey: ["old_card"],
      partition: "detail",
      region: "jp",
      purpose: "engine",
    })
    expect(related).toBe(file)
    await client.fragments(related ?? "")
    const before = requests.length
    const rows = (await client.fragments(file ?? "")).find(
      (fragment) => fragment.table === "card_mentioned_by",
    )?.rows
    expect(rows).toEqual([{ card_id: "old_card", region: "jp", from_card_id: "card" }])
    expect(requests.slice(before)).toEqual([])
    expect(manifest["min_reader_version"]).toBe("4.0.1")
    expect(isCompatible(manifest)).toBe(true)
    expect(isCompatible({ ...manifest, min_reader_version: "4.0.2" })).toBe(false)
  })
  it("keeps JP and EN rows in one shared engine file and rejects a regional container", () => {
    const native = objectValue(v3Fixture("mentioned-by-native.json"))
    const manifest = objectValue(native["manifest"])
    const file = (manifest["files"] as JsonObject[]).find(
      (entry) => entry["key"] === "engine/shared/family-family/0",
    )
    if (!file) throw new Error("missing shared engine")
    const value = objectValue(objectValue(native["payloads"])[stringValue(file["key"])])
    const packed = arrayValue(arrayValue(objectValue(value["tables"])["card_mentioned_by"])[0])
    arrayValue(packed[4]).unshift(["old_card", "en", "card"])
    const count = arrayValue(file["row_counts"])
      .map((raw) => arrayValue(raw))
      .find((row) => row[0] === "card_mentioned_by")
    if (!count) throw new Error("missing back-reference count")
    count[5] = 2
    expect(
      readContainer(file, value).find((fragment) => fragment.table === "card_mentioned_by")?.rows,
    ).toEqual([
      { card_id: "old_card", region: "en", from_card_id: "card" },
      { card_id: "old_card", region: "jp", from_card_id: "card" },
    ])
    expect(() => readContainer({ ...file, region: "jp" }, value)).toThrow(
      "back references require shared engine",
    )
  })
  it("accepts both manifests in the shared mixed reader-minimum window", () => {
    const mixed = arrayValue(v3Fixture("index-cases.json"))
      .map((raw) => objectValue(raw))
      .find((item) => item["name"] === "mixed-reader-minimum-window")
    if (!mixed) throw new Error("missing mixed-window fixture")
    const index = objectValue(mixed["index"])
    const entries = [objectValue(index["current"]), objectValue(index["previous"])]
    expect(entries.map((entry) => entry["min_reader_version"])).toEqual(["4.0.1", "4.0.0"])
    for (const entry of entries) {
      expect(isCompatible(entry)).toBe(true)
      const manifest = objectValue(
        objectValue(mixed["manifests"])[stringValue(entry["manifest_sha256"])],
      )
      expect(readManifest(manifest).manifest["min_reader_version"]).toBe(
        entry["min_reader_version"],
      )
    }
  })
})
