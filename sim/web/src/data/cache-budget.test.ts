// @vitest-environment node
import { expect, it, vi } from "vitest"

import { memoryCache } from "../test-utils/cache"
import { trimDataCaches, VIEW_BUDGET } from "./cache-budget"
import { VerifiedCache } from "./verified-cache"

it("keeps the latest complete view, bounds all view slots together, and isolates another root", async () => {
  const storage = memoryCache()
  const root = "https://cdn.test/root"
  const otherRoot = "https://cdn.test/root-other"
  const first = new VerifiedCache(root, "search-view", 1, storage, "test")
  const other = new VerifiedCache(otherRoot, "search-view", 1, storage, "test")
  const bytes = new Uint8Array(VIEW_BUDGET / 2)
  const hash = `sha256:${"1".repeat(64)}`
  await other.write("default", hash, bytes)
  for (const slot of ["first", "second", "third"]) await first.write(slot, hash, bytes)
  expect(await first.read("first", hash)).toBeUndefined()
  expect((await first.read("second", hash))?.length).toBe(bytes.length)
  expect((await first.read("third", hash))?.length).toBe(bytes.length)
  expect((await other.read("default", hash))?.length).toBe(bytes.length)
})

it("retains only two generations across distinct query slots", async () => {
  const storage = memoryCache()
  const cache = new VerifiedCache("https://cdn.test", "search-view", 1, storage, "test")
  for (const n of [1, 2, 3])
    await cache.write(`query-${String(n)}`, `sha256:${String(n).repeat(64)}`, new Uint8Array([n]))
  expect(await cache.read("query-1", `sha256:${"1".repeat(64)}`)).toBeUndefined()
  expect(await cache.read("query-2", `sha256:${"2".repeat(64)}`)).toEqual(new Uint8Array([2]))
  expect(await cache.read("query-3", `sha256:${"3".repeat(64)}`)).toEqual(new Uint8Array([3]))
})

it("combines raw metadata and decoded records under the same root budget", async () => {
  const storage = memoryCache()
  const root = "https://cdn.test/root"
  const cache = await storage.open(`sve-search-index:${encodeURIComponent(root)}`)
  const hash = `sha256%3A${"1".repeat(64)}`
  const raw = await storage.open(`sve-raw:${encodeURIComponent(root)}`)
  for (const [n, target, url] of [
    [1, raw, `${root}/__sve-raw/${hash}/bytes`],
    [2, cache, `${root}/index/${hash}`],
  ] as const)
    await target.put(
      url,
      new Response("x", {
        headers: { "x-sve-bytes": String(40 * 1024 * 1024), "x-sve-touched": String(n) },
      }),
    )
  await trimDataCaches(storage, root)
  expect((await cache.keys()).length).toBe(1)
  expect(await raw.keys()).toHaveLength(1)
  await raw.put(
    `${root}/extra`,
    new Response("x", {
      headers: { "x-sve-bytes": String(30 * 1024 * 1024), "x-sve-touched": "3" },
    }),
  )
  await trimDataCaches(storage, root)
  expect(await cache.keys()).toHaveLength(1)
  expect(await raw.match(`${root}/__sve-raw/${hash}/bytes`)).toBeUndefined()
  expect(await raw.match(`${root}/extra`)).toBeDefined()
})

it("evicts a legacy headerless record once without reading its body on later scans", async () => {
  const storage = memoryCache()
  const root = "https://cdn.test"
  const cache = await storage.open(`sve-snapshot:${encodeURIComponent(root)}`)
  const url = `${root}/legacy`
  await cache.put(url, new Response(new Uint8Array(32 * 1024 * 1024)))
  const open = storage.open.bind(storage)
  const namespace = `sve-snapshot:${encodeURIComponent(root)}`
  vi.spyOn(storage, "open").mockImplementation((name) =>
    name === namespace ? Promise.resolve(cache) : open(name),
  )
  const matched = vi.spyOn(cache, "match")
  const removed = vi.spyOn(cache, "delete")
  const read = vi.spyOn(Response.prototype, "arrayBuffer")
  try {
    await trimDataCaches(storage, root)
    expect(await cache.keys()).toEqual([])
    await trimDataCaches(storage, root)
    expect(matched).toHaveBeenCalledTimes(1)
    expect(removed).toHaveBeenCalledTimes(1)
    expect(read).not.toHaveBeenCalled()
  } finally {
    read.mockRestore()
  }
})
