// @vitest-environment node
import { expect, it, vi } from "vitest"

import { memoryCache } from "../test-utils/cache"
import { dataCacheBytes, readDataCacheBudget } from "./cache-budget"
import type { Fetcher } from "./cdn"
import { transferDigest } from "./integrity"
import { MetadataBytes } from "./metadata"
import { rawBlobDescriptor, RawBlobStore, rawRetentionPlan } from "./raw-blobs"
import { appRequests } from "./request-queue"
import { mainNetwork, type NetworkMessage, workerFetch } from "./search/network"

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
  const fetcher = vi.fn<Fetcher>(() => Promise.resolve(new Response(payload)))
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
  expect(background).toHaveBeenCalledWith(
    `${base}/${file.path}`,
    expect.objectContaining({ priority: "low" }),
  )
})

it("shares raw identity across worker, metadata and detail subscribers with independent cancellation", async () => {
  const gate = Promise.withResolvers<undefined>()
  const entered = Promise.withResolvers<undefined>()
  const fetcher = vi.fn(async (_url: string, init?: RequestInit) => {
    entered.resolve(undefined)
    await gate.promise
    init?.signal?.throwIfAborted()
    return new Response(payload)
  })
  vi.stubGlobal("fetch", fetcher)
  const storage = memoryCache()
  const worker = workerFetch((message) => {
    bridge.receive(message)
  }, false)
  const bridge = mainNetwork({
    postMessage: (message: NetworkMessage) => {
      if (message.kind === "fetch-reply") worker.receive(message)
    },
  } as Worker)
  const cancel = new AbortController()
  const background = new RawBlobStore(base, worker.fetch, storage)
  const pending = background.read(rawBlobDescriptor(file), { signal: cancel.signal })
  const rejected = expect(pending).rejects.toThrow("cancelled")
  await entered.promise
  const renamed = { ...file, path: "another-path.json" }
  const metadata = new MetadataBytes(
    base,
    new Map([[file.key, renamed]]),
    appRequests.foreground,
    () => undefined,
    storage,
    appRequests.background,
  )
  const visible = metadata.read(file.key)
  const retained = new RawBlobStore(base, appRequests.foreground, storage).read(
    rawBlobDescriptor(renamed),
  )
  // Let cache probes attach the two foreground subscribers before cancelling the worker owner.
  await new Promise((resolve) => setTimeout(resolve, 0))
  cancel.abort()
  await rejected
  gate.resolve(undefined)
  try {
    expect(await visible).toEqual(payload)
    expect(await retained).toEqual(payload)
    expect(fetcher).toHaveBeenCalledTimes(1)
  } finally {
    metadata.dispose()
    bridge.dispose()
    vi.unstubAllGlobals()
  }
})

it("recovers persistence only on a successful write without a newer concurrent storage failure", async () => {
  const storage = memoryCache()
  const name = `sve-raw:${encodeURIComponent(base)}`
  const cache = await storage.open(name)
  const open = storage.open.bind(storage)
  storage.open = (key) => (key === name ? Promise.resolve(cache) : open(key))
  const fetcher = vi.fn<Fetcher>(() => Promise.resolve(new Response(payload)))
  const store = new RawBlobStore(base, fetcher, storage)
  await store.read(rawBlobDescriptor(file))
  expect(store.persistent).toBe(true)
  const nextBytes = new Uint8Array([4, 5, 6])
  const next = { ...file, sha256: await transferDigest(nextBytes) }
  const put = cache.put.bind(cache)
  const pending = Promise.withResolvers<undefined>()
  const entered = Promise.withResolvers<undefined>()
  let block = true
  cache.put = async (key, response) => {
    const url = typeof key === "string" ? key : key instanceof URL ? key.href : key.url
    if (url.includes(encodeURIComponent(file.sha256))) {
      if (block) {
        entered.resolve(undefined)
        await pending.promise
      }
      await put(key, response)
    } else throw new Error("quota")
  }
  const slot = `${base}/__sve-raw/${encodeURIComponent(file.sha256)}/bytes`
  await cache.delete(slot)
  fetcher.mockImplementation((url) =>
    Promise.resolve(new Response(url.endsWith("later.json") ? nextBytes : payload)),
  )
  const earlier = store.read(rawBlobDescriptor(file))
  await entered.promise
  await store.read(rawBlobDescriptor({ ...next, path: "later.json" }))
  expect(store.persistent).toBe(false)
  pending.resolve(undefined)
  await earlier
  expect(store.persistent).toBe(false)
  await store.read(rawBlobDescriptor(file))
  expect(store.persistent).toBe(false)
  block = false
  await cache.delete(slot)
  await store.read(rawBlobDescriptor(file))
  expect(store.persistent).toBe(true)
})

it("collects only unreferenced hashes from whole retained manifests and recovers a two-tab GC miss", async () => {
  const storage = memoryCache()
  const bodies = [payload, new Uint8Array([4]), new Uint8Array([5])]
  const descriptors = await Promise.all(
    bodies.map(async (bytes, index) => ({
      ...file,
      key: String(index),
      path: `${String(index)}.json`,
      bytes: bytes.length,
      sha256: await transferDigest(bytes),
    })),
  )
  const fetcher = vi.fn<Fetcher>((url) =>
    Promise.resolve(
      new Response(bodies[Number(url.split("/").at(-1)?.split(".")[0])]?.slice().buffer),
    ),
  )
  const tab = new RawBlobStore(base, fetcher, storage)
  for (const descriptor of descriptors) await tab.read(descriptor)
  const [old, detail, current] = descriptors
  if (!old || !detail || !current) throw new Error("missing descriptors")
  await tab.retain(
    new Map([
      [old.key, old],
      [detail.key, detail],
    ]),
    `sha256:${"1".repeat(64)}`,
  )
  await tab.retain(
    new Map([
      [current.key, current],
      [detail.key, detail],
    ]),
    `sha256:${"2".repeat(64)}`,
  )
  expect(await tab.has(old)).toBe(true)
  expect(await tab.has(detail)).toBe(true)
  // A second tab's later adoption drops the oldest generation; an already-probed read must miss safely.
  const second = new RawBlobStore(base, fetcher, storage)
  await second.retain(new Map([[detail.key, detail]]), `sha256:${"3".repeat(64)}`)
  expect(await tab.has(old)).toBe(false)
  expect(await tab.read(old)).toEqual(payload)
  expect(await second.has(detail)).toBe(true)
  expect(fetcher.mock.calls.filter(([url]) => url.endsWith(old.path))).toHaveLength(2)
  // A damaged plan fails closed, and re-retaining the same hash repairs it.
  const plans = await storage.open(`sve-retained:${encodeURIComponent(base)}`)
  const damaged = (await plans.keys()).find((key) =>
    key.url.includes(encodeURIComponent(`sha256:${"2".repeat(64)}`)),
  )
  if (!damaged) throw new Error("missing plan")
  await plans.put(
    damaged.url,
    new Response("x", {
      headers: { "x-sve-sha256": "bad", "x-sve-bytes": "1", "x-sve-touched": "1" },
    }),
  )
  await expect(readDataCacheBudget(storage, base)).rejects.toThrow("damaged")
  // Re-adopting a retained hash makes it the newest plan, so the next adoption drops the other one.
  await tab.read(current)
  await tab.retain(
    new Map([
      [current.key, current],
      [detail.key, detail],
    ]),
    `sha256:${"2".repeat(64)}`,
  )
  await expect(readDataCacheBudget(storage, base)).resolves.toBeDefined()
  await tab.retain(new Map([[old.key, old]]), `sha256:${"4".repeat(64)}`)
  expect(await tab.has(current)).toBe(true)
  expect(await tab.has(detail)).toBe(true)
})

it("preserves necessary old bytes when activation or a quota retry fails", async () => {
  const storage = memoryCache()
  const fetcher = vi.fn<Fetcher>(() => Promise.resolve(new Response(payload)))
  const tab = new RawBlobStore(base, fetcher, storage)
  await tab.read(file)
  await tab.retain(new Map([[file.key, file]]), `sha256:${"1".repeat(64)}`)
  const open = storage.open.bind(storage)
  const raw = await storage.open(`sve-raw:${encodeURIComponent(base)}`)
  const plans = await storage.open(`sve-retained:${encodeURIComponent(base)}`)
  storage.open = (name) =>
    name.startsWith("sve-retained:")
      ? Promise.resolve(plans)
      : name.startsWith("sve-raw:")
        ? Promise.resolve(raw)
        : open(name)
  vi.spyOn(plans, "put").mockRejectedValue(new Error("quota"))
  await tab.retain(new Map(), `sha256:${"2".repeat(64)}`)
  expect(await tab.has(file)).toBe(true)
  const put = vi.spyOn(raw, "put").mockRejectedValue(new Error("quota"))
  const nextPayload = new Uint8Array([8])
  const next = { ...file, sha256: await transferDigest(nextPayload), bytes: 1 }
  const nextTab = new RawBlobStore(base, () => Promise.resolve(new Response(nextPayload)), storage)
  await nextTab.read(next)
  expect(put.mock.calls.length).toBeLessThanOrEqual(2)
  expect(nextTab.persistent).toBe(false)
  expect(await tab.read(file)).toEqual(payload)
  expect(fetcher).toHaveBeenCalledTimes(1)
})
