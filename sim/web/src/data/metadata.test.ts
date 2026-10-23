// @vitest-environment node
import { describe, expect, it, vi } from "vitest"

import { memoryCache } from "../test-utils/cache"
import { dataCacheBytes } from "./cache-budget"
import type { Fetcher } from "./cdn"
import type { JsonObject } from "./format-v3/json"
import { transferDigest } from "./integrity"
import { MetadataBytes } from "./metadata"
import { rawBlobDescriptor, RawBlobStore } from "./raw-blobs"
import { SearchBlobs } from "./search/blobs"

const tick = () => new Promise<void>((resolve) => setTimeout(resolve, 0))
const MiB = 1024 * 1024

it("shares metadata raw bytes with search and detail readers across manifests and clearing", async () => {
  const storage = memoryCache()
  const { files, fetcher, bytes } = await fixture([3], storage)
  const file = files.get("file0")
  if (!file) throw new Error("fixture missing")
  const renamed = { ...file, key: "new-key", path: "new-path" }
  await bytes.prefetch()
  expect(bytes.status()).toMatchObject({ state: "complete", done: 1, persistent: true })
  const search = new SearchBlobs("/cdn", fetcher, storage)
  await search.read(renamed, new AbortController().signal)
  const detail = new RawBlobStore("/cdn", fetcher, storage)
  await detail.read(rawBlobDescriptor(renamed))
  bytes.dispose()
  const next = new MetadataBytes(
    "/cdn",
    new Map([["new-key", renamed]]),
    fetcher,
    () => undefined,
    storage,
  )
  await next.prefetch()
  expect(next.status()).toMatchObject({ state: "complete", done: 1, persistent: true })
  expect(fetcher).toHaveBeenCalledTimes(1)
  expect(await dataCacheBytes(storage)).toBe(3)
  const raw = await storage.open("sve-raw:%2Fcdn")
  expect(await raw.keys()).toHaveLength(1)
  await storage.delete("sve-raw:%2Fcdn")
  await detail.read(rawBlobDescriptor(file))
  expect(fetcher).toHaveBeenCalledTimes(2)
  expect(await dataCacheBytes(storage)).toBe(3)
  next.dispose()
})

async function fixture(sizes: readonly number[], storage?: CacheStorage) {
  const bodies = new Map<number, Uint8Array>()
  const hashes = new Map<number, string>()
  for (const [index, size] of sizes.entries()) {
    const body = new Uint8Array(size).fill(index + 7)
    bodies.set(index, body)
    hashes.set(index, await transferDigest(body))
  }
  const files = new Map<string, JsonObject>(
    sizes.map((size, index) => [
      `file${String(index)}`,
      {
        key: `file${String(index)}`,
        path: `data${String(index)}`,
        bytes: size,
        sha256: hashes.get(index) ?? "",
      },
    ]),
  )
  const response = (url: string) => {
    const body = bodies.get(Number(url.split("data")[1]))
    if (!body) throw new Error("unexpected request")
    return new Response(body.slice().buffer)
  }
  const fetcher = vi.fn<Fetcher>((url) => Promise.resolve(response(url)))
  const bytes = new MetadataBytes("/cdn", files, fetcher, () => undefined, storage)
  return { files, response, fetcher, bytes }
}

describe("metadata library boundaries", () => {
  it("does not retain settled raw bytes in RAM when persistent storage is unavailable", async () => {
    const { bytes, fetcher } = await fixture(Array.from({ length: 65 }, () => 1))
    for (let i = 0; i < 64; i += 1) await bytes.read(`file${String(i)}`)
    await bytes.read("file0")
    await bytes.read("file64")
    await bytes.read("file0")
    expect(bytes.status()).toMatchObject({ state: "idle", done: 0, persistent: false })
    expect(fetcher).toHaveBeenCalledTimes(67)
    await bytes.read("file1")
    expect(fetcher).toHaveBeenCalledTimes(68)
    bytes.dispose()
  })

  it("releases large foreground bytes rather than creating a RAM fallback", async () => {
    const { bytes, fetcher } = await fixture([6 * MiB, 6 * MiB, 1])
    await bytes.read("file0")
    await bytes.read("file1")
    await bytes.read("file0")
    expect(fetcher).toHaveBeenCalledTimes(3)
    await bytes.read("file2")
    await bytes.read("file0")
    expect(fetcher).toHaveBeenCalledTimes(5)
    await bytes.read("file1")
    expect(fetcher).toHaveBeenCalledTimes(6)
    bytes.dispose()
  })

  it("releases oversized and empty bodies after each on-demand read", async () => {
    const { bytes, fetcher } = await fixture([13 * MiB, 0])
    await bytes.read("file0")
    await tick()
    await bytes.read("file0")
    expect(fetcher).toHaveBeenCalledTimes(2)
    await bytes.read("file1")
    await bytes.read("file1")
    expect(fetcher).toHaveBeenCalledTimes(4)
    bytes.dispose()
  })

  it("shares the exact in-flight promise and continues after a failed transport", async () => {
    const { files, response } = await fixture([1, 1])
    const gate = Promise.withResolvers<Response>()
    const fetcher = vi.fn<Fetcher>(() => gate.promise)
    const bytes = new MetadataBytes("/cdn", files, fetcher, () => undefined)
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
    const bytes = new MetadataBytes("/cdn", files, fetcher, () => undefined, memoryCache())
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
    const bytes = new MetadataBytes("/cdn", files, fetcher, () => undefined, memoryCache())
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
    const bytes = new MetadataBytes("/cdn", files, fetcher, () => undefined)
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

it("rejects a cancelled scoped image read and ignores bytes arriving after cancellation", async () => {
  const storage = memoryCache()
  const { files, response } = await fixture([1], storage)
  const gate = Promise.withResolvers<Response>()
  const background = vi.fn<Fetcher>(() => gate.promise)
  const bytes = new MetadataBytes(
    "/cdn",
    files,
    (url) => Promise.resolve(response(url)),
    () => undefined,
    storage,
    background,
  )
  const abort = new AbortController()
  const reading = bytes.read("file0", false, abort.signal)
  const rejected = expect(reading).rejects.toThrow()
  await vi.waitFor(() => {
    expect(background).toHaveBeenCalledTimes(1)
  })
  abort.abort()
  await rejected
  gate.resolve(response("/cdn/data0"))
  await tick()
  const cache = await storage.open("sve-raw:%2Fcdn")
  expect(await cache.keys()).toHaveLength(0)
  expect(await bytes.read("file0")).toHaveLength(1)
  bytes.dispose()
  await expect(bytes.read("file0", false, new AbortController().signal)).rejects.toThrow(
    "snapshot replaced",
  )
})
