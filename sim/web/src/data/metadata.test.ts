// @vitest-environment node
import { describe, expect, it, vi } from "vitest"

import { memoryCache } from "../test-utils/cache"
import type { Fetcher } from "./cdn"
import type { JsonObject } from "./format-v3/json"
import { transferDigest } from "./integrity"
import { MetadataBytes } from "./metadata"

const tick = () => new Promise<void>((resolve) => setTimeout(resolve, 0))
const MiB = 1024 * 1024

async function fixture(sizes: readonly number[], storage?: CacheStorage) {
  const bodies = new Map<number, Uint8Array>()
  const hashes = new Map<number, string>()
  for (const size of new Set(sizes)) {
    const body = new Uint8Array(size).fill(7)
    bodies.set(size, body)
    hashes.set(size, await transferDigest(body))
  }
  const files = new Map<string, JsonObject>(
    sizes.map((size, index) => [
      `file${String(index)}`,
      { path: `data${String(index)}`, bytes: size, sha256: hashes.get(size) ?? "" },
    ]),
  )
  const response = (url: string) => {
    const size = sizes[Number(url.split("data")[1])]
    const body = size === undefined ? undefined : bodies.get(size)
    if (!body) throw new Error("unexpected request")
    return new Response(body.slice().buffer)
  }
  const fetcher = vi.fn<Fetcher>((url) => Promise.resolve(response(url)))
  const bytes = new MetadataBytes("/cdn", "synthetic", files, fetcher, () => undefined, storage)
  return { files, response, fetcher, bytes }
}

describe("metadata library boundaries", () => {
  it("caps fallback RAM at 64 files and refreshes LRU on reads", async () => {
    const { bytes, fetcher } = await fixture(Array.from({ length: 65 }, () => 1))
    for (let i = 0; i < 64; i += 1) await bytes.read(`file${String(i)}`)
    await bytes.read("file0")
    await bytes.read("file64")
    await bytes.read("file0")
    expect(fetcher).toHaveBeenCalledTimes(65)
    await bytes.read("file1")
    expect(fetcher).toHaveBeenCalledTimes(66)
    bytes.dispose()
  })

  it("retains exactly 12 MiB and evicts the least recent file when bytes exceed it", async () => {
    const { bytes, fetcher } = await fixture([6 * MiB, 6 * MiB, 1])
    await bytes.read("file0")
    await bytes.read("file1")
    await bytes.read("file0")
    expect(fetcher).toHaveBeenCalledTimes(2)
    await bytes.read("file2")
    await bytes.read("file0")
    expect(fetcher).toHaveBeenCalledTimes(3)
    await bytes.read("file1")
    expect(fetcher).toHaveBeenCalledTimes(4)
    bytes.dispose()
  })

  it("does not retain oversized values and accepts empty bodies without a size exception", async () => {
    const { bytes, fetcher } = await fixture([13 * MiB, 0])
    await bytes.read("file0")
    await tick()
    await bytes.read("file0")
    expect(fetcher).toHaveBeenCalledTimes(2)
    await bytes.read("file1")
    await bytes.read("file1")
    expect(fetcher).toHaveBeenCalledTimes(3)
    bytes.dispose()
  })

  it("shares the exact in-flight promise and continues after a failed transport", async () => {
    const { files, response } = await fixture([1, 1])
    const gate = Promise.withResolvers<Response>()
    const fetcher = vi.fn<Fetcher>(() => gate.promise)
    const bytes = new MetadataBytes("/cdn", "dedup", files, fetcher, () => undefined)
    const first = bytes.read("file0")
    expect(bytes.read("file0")).toBe(first)
    const rejected = expect(first).rejects.toThrow("failed")
    gate.reject(new Error("failed"))
    await rejected
    await tick()
    fetcher.mockImplementation((url) => Promise.resolve(response(url)))
    await bytes.read("file0")
    expect(fetcher).toHaveBeenCalledTimes(2)
    bytes.dispose()
  })

  it("promotes an admitted background job waiting behind four foreground requests", async () => {
    const { files, response } = await fixture(Array.from({ length: 10 }, () => 1))
    const starts: string[] = []
    const gates = new Map<string, ReturnType<typeof Promise.withResolvers<Response>>>()
    const fetcher: Fetcher = (url) => {
      starts.push(url)
      const gate = Promise.withResolvers<Response>()
      gates.set(url, gate)
      return gate.promise
    }
    const bytes = new MetadataBytes(
      "/cdn",
      "admitted",
      files,
      fetcher,
      () => undefined,
      memoryCache(),
    )
    const foreground = [0, 1, 2, 3].map((i) => bytes.read(`file${String(i)}`))
    await tick()
    const prefetch = bytes.prefetch()
    await tick()
    expect(starts).toHaveLength(4)
    const promoted = bytes.read("file6")
    expect(bytes.read("file6")).toBe(promoted)
    gates.get("/cdn/data0")?.resolve(response("/cdn/data0"))
    await foreground[0]
    await vi.waitFor(
      () => {
        expect(starts[4]).toBe("/cdn/data6")
      },
      { interval: 1 },
    )
    bytes.cancel()
    for (const [url, gate] of gates) gate.resolve(response(url))
    await Promise.all([...foreground, promoted, prefetch])
    expect(starts).toHaveLength(5)
    bytes.dispose()
  })

  it("does not let cancelled cleanup erase a replacement request for the same key", async () => {
    const { files, response } = await fixture([1, 1, 1, 1])
    const gate = Promise.withResolvers<undefined>()
    const fetcher = vi.fn<Fetcher>(async (url) => {
      await gate.promise
      return response(url)
    })
    const bytes = new MetadataBytes(
      "/cdn",
      "replacement",
      files,
      fetcher,
      () => undefined,
      memoryCache(),
    )
    const prefetch = bytes.prefetch()
    await tick()
    expect(fetcher).toHaveBeenCalledTimes(3)
    bytes.cancel()
    const promoted = bytes.read("file3")
    await tick()
    expect(bytes.read("file3")).toBe(promoted)
    gate.resolve(undefined)
    await Promise.all([prefetch, promoted])
    expect(fetcher).toHaveBeenCalledTimes(4)
    bytes.dispose()
  })

  it("rejects queued foreground work on disposal without starting it", async () => {
    const { files } = await fixture([1, 1, 1, 1, 1])
    let aborted = 0
    const fetcher = vi.fn<Fetcher>(
      (_url, init) =>
        new Promise((_resolve, reject) => {
          init?.signal?.addEventListener("abort", () => {
            aborted += 1
            reject(new Error("aborted"))
          })
        }),
    )
    const bytes = new MetadataBytes("/cdn", "dispose", files, fetcher, () => undefined)
    const reads = [0, 1, 2, 3, 4].map((i) => bytes.read(`file${String(i)}`))
    const results = Promise.allSettled(reads)
    await tick()
    expect(fetcher).toHaveBeenCalledTimes(4)
    bytes.dispose()
    expect((await results).every((result) => result.status === "rejected")).toBe(true)
    expect(fetcher).toHaveBeenCalledTimes(4)
    expect(aborted).toBe(4)
    expect(bytes.status().done).toBe(0)
  })
})
