// @vitest-environment node
import { expect, it, vi } from "vitest"

import { memoryCache } from "../test-utils/cache"
import { dataCacheBytes } from "./cache-budget"
import { transferDigest } from "./integrity"
import { rawBlobDescriptor, RawBlobStore, rawRetentionPlan } from "./raw-blobs"

const base = "https://raw.test/cdn"
const payload = new Uint8Array([1, 2, 3])
const file = {
  key: "detail/jp/shared/0",
  path: "snapshots/blobs/first.json",
  sha256: await transferDigest(payload),
  bytes: payload.length,
}

it("shares immutable bytes across retained manifests and releases completed reads", async () => {
  const storage = memoryCache()
  const renamed = { ...file, key: "detail/jp/shared/1", path: "snapshots/blobs/next.json" }
  const manifests = [
    { hash: "old", files: new Map([[file.key, file]]) },
    { hash: "new", files: new Map([[renamed.key, renamed]]) },
  ]
  const plan = rawRetentionPlan(manifests)
  expect(plan.size).toBe(1)
  expect(plan.get(file.sha256)).toEqual({
    bytes: payload.length,
    manifests: new Set(["old", "new"]),
  })
  expect(rawRetentionPlan(manifests.slice(1)).get(file.sha256)?.manifests).toEqual(new Set(["new"]))
  expect(() =>
    rawRetentionPlan([
      ...manifests,
      { hash: "invalid", files: new Map([[file.key, { ...file, bytes: 99 }]]) },
    ]),
  ).toThrow("conflicting raw lengths")
  const gate = Promise.withResolvers<undefined>()
  const fetcher = vi.fn(async () => {
    await gate.promise
    return new Response(payload, { headers: { "content-encoding": "br", "content-length": "1" } })
  })
  const store = new RawBlobStore(base, fetcher, storage)
  const first = store.read(rawBlobDescriptor(file))
  const concurrent = store.read(rawBlobDescriptor(renamed))
  gate.resolve(undefined)
  expect(await first).toEqual(payload)
  expect(await concurrent).toEqual(payload)
  expect(fetcher).toHaveBeenCalledTimes(1)
  const next = new RawBlobStore(base, fetcher, storage)
  expect(await next.read(rawBlobDescriptor(renamed))).toEqual(payload)
  expect(fetcher).toHaveBeenCalledTimes(1)
  const names = await storage.keys()
  const cache = await storage.open(names[0] ?? "missing")
  const keys = await cache.keys()
  expect(keys).toHaveLength(1)
  const response = keys[0] && (await cache.match(keys[0]))
  expect(response?.headers.get("content-length")).toBe(String(payload.length))
  expect(response?.headers.get("content-encoding")).toBeNull()
  expect(await dataCacheBytes(storage)).toBe(payload.length)
  await storage.delete(names[0] ?? "missing")
  expect(await store.read(rawBlobDescriptor(file))).toEqual(payload)
  expect(fetcher).toHaveBeenCalledTimes(2)
  expect(await dataCacheBytes(storage)).toBe(payload.length)
})

it("repairs corrupt bytes and falls back to verified on-demand reads when storage fails", async () => {
  const descriptor = rawBlobDescriptor(file)
  const storage = memoryCache()
  const fetcher = vi.fn(() => Promise.resolve(new Response(payload)))
  const store = new RawBlobStore(base, fetcher, storage)
  await store.read(descriptor)
  const cache = await storage.open((await storage.keys())[0] ?? "missing")
  const key = (await cache.keys())[0]
  if (!key) throw new Error("raw bytes missing")
  for (const damaged of [new Uint8Array([9, 9, 9]), new Uint8Array([1])]) {
    await cache.put(key, new Response(damaged))
    expect(await store.read(descriptor)).toEqual(payload)
  }
  expect(fetcher).toHaveBeenCalledTimes(3)
  const refused = memoryCache()
  refused.open = () => Promise.reject(new Error("denied"))
  for (const unavailable of [memoryCache(true), refused, undefined]) {
    const network = vi.fn(() => Promise.resolve(new Response(payload)))
    const fallback = new RawBlobStore(base, network, unavailable)
    expect(await fallback.read(descriptor)).toEqual(payload)
    expect(await fallback.read(descriptor)).toEqual(payload)
    expect(network).toHaveBeenCalledTimes(2)
  }
  const retry = vi
    .fn()
    .mockResolvedValueOnce(new Response(new Uint8Array([0])))
    .mockImplementation(() => Promise.resolve(new Response(payload)))
  expect(await new RawBlobStore(base, retry).read(descriptor)).toEqual(payload)
  expect(retry).toHaveBeenCalledTimes(2)
  const corrupt = vi.fn(() => Promise.resolve(new Response(new Uint8Array([9, 9, 9]))))
  await expect(new RawBlobStore(base, corrupt).read(descriptor)).rejects.toMatchObject({
    code: "blob-integrity",
  })
  expect(corrupt).toHaveBeenCalledTimes(2)
  const aborted = new AbortController()
  aborted.abort(new Error("cancelled"))
  const foreground = vi.fn(() => Promise.resolve(new Response(payload)))
  const background = vi.fn(() => Promise.resolve(new Response(payload)))
  const scheduled = new RawBlobStore(base, foreground, undefined, background)
  await expect(scheduled.read(descriptor, { signal: aborted.signal })).rejects.toThrow("cancelled")
  expect(foreground).not.toHaveBeenCalled()
  expect(await scheduled.read(descriptor, { priority: "low" })).toEqual(payload)
  expect(background).toHaveBeenCalledWith(`${base}/${file.path}`, { priority: "low" })
})
