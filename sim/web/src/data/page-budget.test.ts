// @vitest-environment node
import { describe, expect, it, vi } from "vitest"

import type { LoadedSnapshot, SnapshotClient } from "./client"
import type { JsonObject } from "./format-v1/json"
import type { Fragment } from "./format-v1/reader"
import { bucket } from "./format-v1/sha256"
import { loadImagePage } from "./images"

function page(extra: number) {
  const ids: string[] = []
  const buckets = new Set<number>()
  for (let i = 0; ids.length < 24; i += 1) {
    const id = `image:${String(i)}`
    const number = bucket([id], 64)
    if (!buckets.has(number)) {
      ids.push(id)
      buckets.add(number)
    }
  }
  const faces = ids.map((_, i) => ({ printingId: `p:${String(i)}`, faceId: `f:${String(i)}` }))
  const owner = { kind: "home_set", id: "TEST" }
  const global = { kind: "global", id: null }
  const bindingKey = "images/detail/home_set/TEST/band/0"
  const files = new Map<string, JsonObject>()
  const fragments = new Map<string, Fragment[]>()
  const bindings = faces.map((face, i) => ({
    printing_id: face.printingId,
    face_id: face.faceId,
    image_id: ids[i] ?? "",
  }))
  const printing: Fragment = {
    file: "bootstrap",
    table: "printing",
    identity: "printing",
    value: { owner, base: null },
    rows: faces.map((face) => ({ id: face.printingId })),
  }
  files.set(bindingKey, {
    key: bindingKey,
    role: "images",
    bytes: 503316 + extra,
    row_counts: faces.map((face) => ({
      table: "printing_image",
      partition: "detail",
      owner,
      bucket: bucket([face.printingId], 64),
    })),
  })
  fragments.set(bindingKey, [
    {
      file: bindingKey,
      table: "printing_image",
      identity: "binding",
      value: { owner, base: null },
      rows: bindings,
    },
  ])
  for (const id of ids) {
    const number = bucket([id], 64)
    const key = `images/detail/global/global/band/${String(number)}`
    files.set(key, {
      key,
      role: "images",
      bytes: 503316,
      row_counts: ["image_asset", "image_variant"].map((table) => ({
        table,
        partition: "detail",
        owner: global,
        bucket: number,
      })),
    })
    fragments.set(key, [
      {
        file: key,
        table: "image_asset",
        identity: id,
        value: { owner: global, base: null },
        rows: [{ id, publication_state: "approved", availability: "available" }],
      },
    ])
  }
  // The page API consumes an already-validated client, independently of byte decoding.
  const snapshot = {
    manifest: { format_version: "1.1.0", partitioning: { bucket_count: 64 } },
    bootstrap: [printing],
    files,
  } as unknown as LoadedSnapshot
  const read = vi.fn((key: string) => Promise.resolve(fragments.get(key) ?? []))
  const client = {
    base: "/cdn",
    snapshot: () => snapshot,
    fragments: read,
  } as unknown as SnapshotClient
  return { client, faces, read }
}

describe("image page raw budget", () => {
  it("accepts a 24-face workset just under 12 MiB and rejects the same valid-sized files one byte above", async () => {
    const fits = page(0)
    expect((await loadImagePage(fits.client, fits.faces)).asset("p:0", "f:0")).toBeDefined()
    expect(fits.read).toHaveBeenCalledTimes(25)
    const oversized = page(13)
    await expect(loadImagePage(oversized.client, oversized.faces)).rejects.toThrow(
      "image page exceeds 12 MiB",
    )
    expect(oversized.read).toHaveBeenCalledTimes(24)
  })
})
