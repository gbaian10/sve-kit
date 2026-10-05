// @vitest-environment node
import { describe, expect, it, vi } from "vitest"

import type { LoadedSnapshot, SnapshotClient } from "./client"
import type { JsonObject } from "./format-v1/json"
import type { Fragment } from "./format-v1/reader"
import { bucket } from "./format-v1/sha256"
import { loadImagePage } from "./images"

function page(extra: number) {
  const faces = Array.from({ length: 25 }, (_, i) => ({
    printingId: `p:${String(i)}`,
    faceId: `f:${String(i)}`,
  }))
  const files = new Map<string, JsonObject>()
  const fragments = new Map<string, Fragment[]>()
  const bootstrap: Fragment[] = []
  for (const [i, face] of faces.entries()) {
    const owner = { kind: "home_set", id: `set:${String(i)}` }
    const number = bucket([face.printingId], 64)
    const key = `images/detail/home_set/set%3A${String(i)}/band/${String(Math.floor(number / 32))}`
    files.set(key, {
      key,
      role: "images",
      bytes: 503316 + (i === 0 ? extra : 0),
      row_counts: [{ table: "printing_image", partition: "detail", owner, bucket: number }],
    })
    bootstrap.push({
      file: "bootstrap",
      table: "printing",
      identity: face.printingId,
      value: { owner },
      rows: [
        {
          id: face.printingId,
          int_id: i + 1,
          card_id: `c:${String(i)}`,
          faces: [{ face_id: face.faceId }],
        },
      ],
    })
    bootstrap.push({
      file: "bootstrap",
      table: "face",
      identity: face.faceId,
      value: { owner },
      rows: [{ id: face.faceId, card_id: `c:${String(i)}`, ordinal: 0 }],
    })
    fragments.set(key, [
      {
        file: key,
        table: "printing_image",
        identity: face.printingId,
        value: { owner, base: null },
        rows: [
          {
            printing_id: face.printingId,
            face_id: face.faceId,
            image_id: `i:${String(i)}`,
            publication_state: "approved",
            availability: "available",
            card_version: 1,
            art_version: 1,
            variants: ["art_m", "art_s", "card_l", "card_m", "card_s"].map((size_key) => ({
              size_key,
              width: 128,
              height: 179,
            })),
          },
        ],
      },
    ])
  }
  // The page API consumes an already-validated client, independently of byte decoding.
  const snapshot = {
    manifest: { format_version: "2.0.0", partitioning: { bucket_count: 64 } },
    bootstrap,
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
  it("accepts a 25-face workset just under 12 MiB and rejects the same valid-sized files one byte above", async () => {
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
