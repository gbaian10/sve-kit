// @vitest-environment node
import { afterEach, expect, it, vi } from "vitest"

import { memoryCache } from "../test-utils/cache"
import { dataCacheBytes } from "./cache-budget"
import { clearManagedCaches, estimateStorage } from "./storage"

afterEach(() => {
  vi.unstubAllGlobals()
})

it("estimates origin and both payload categories independently without trimming data", async () => {
  const storage = memoryCache()
  vi.stubGlobal("caches", storage)
  vi.stubGlobal("navigator", { storage: { estimate: () => Promise.resolve({ usage: 999 }) } })
  const images = await storage.open("sve-image-responses-v2")
  await images.put("https://cdn.test/images/card_l/1.webp?v=1", new Response("image"))
  const data = await storage.open("sve-search-index:root")
  await data.put(
    "https://cdn.test/data",
    new Response("x", { headers: { "x-sve-bytes": "500000000" } }),
  )
  expect(await estimateStorage()).toEqual({ total: 999, images: 5, data: 500000000 })
  expect(await data.keys()).toHaveLength(1)
  await data.put("https://cdn.test/unknown", new Response("legacy"))
  expect(await estimateStorage()).toEqual({ total: 999, images: 5, data: undefined })
  expect(await data.keys()).toHaveLength(2)
  await expect(dataCacheBytes(storage)).rejects.toThrow("unavailable")
})

it("reports unavailable APIs and cache read failures as unknown, while empty caches count as zero", async () => {
  vi.stubGlobal("caches", memoryCache())
  vi.stubGlobal("navigator", {})
  expect(await estimateStorage()).toEqual({ total: undefined, images: 0, data: 0 })
  vi.stubGlobal("caches", { keys: () => Promise.reject(new Error("denied")) })
  expect(await estimateStorage()).toEqual({ total: undefined, images: undefined, data: undefined })
})

it("clears managed image, snapshot, metadata and derived search caches but retains unrelated storage", async () => {
  const storage = memoryCache()
  vi.stubGlobal("caches", storage)
  const owned = [
    "sve-image-responses-v2",
    "sve-snapshot:root",
    "sve-images-root-hash",
    "sve-search-index:root",
    "sve-search-blobs:root",
    "sve-search-view:root",
    "sve-search-selection:root",
  ]
  for (const name of [...owned, "another-app"])
    await (await storage.open(name)).put("https://cache.test/item", new Response("old"))
  await clearManagedCaches()
  expect(await storage.keys()).toEqual(["another-app"])
})

it("reports failed or incomplete clearing", async () => {
  const storage = memoryCache()
  vi.stubGlobal("caches", storage)
  await storage.open("sve-snapshot:root")
  vi.spyOn(storage, "delete").mockResolvedValue(false)
  await expect(clearManagedCaches()).rejects.toThrow("incomplete")
  vi.spyOn(storage, "delete").mockRejectedValue(new Error("denied"))
  await expect(clearManagedCaches()).rejects.toThrow("incomplete")
})
