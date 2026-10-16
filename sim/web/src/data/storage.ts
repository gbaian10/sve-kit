import { dataCacheBytes, isDataCache } from "./cache-budget"
import { IMAGE_CACHE } from "./image-cache"

export interface StorageEstimate {
  readonly total: number | undefined
  readonly images: number | undefined
  readonly data: number | undefined
}

async function imageCacheBytes(storage: CacheStorage): Promise<number> {
  if (!(await storage.keys()).includes(IMAGE_CACHE)) return 0
  const cache = await storage.open(IMAGE_CACHE)
  let total = 0
  for (const key of await cache.keys()) {
    const response = await cache.match(key)
    if (!response || response.type === "opaque") throw new Error("Image payload size unavailable")
    total += (await response.arrayBuffer()).byteLength
  }
  return total
}

/** Call on settings entry or explicit refresh: counting images reads their payloads. */
export async function estimateStorage(): Promise<StorageEstimate> {
  const [total, images, data] = await Promise.allSettled([
    Promise.resolve().then(async () => {
      const usage = (await navigator.storage.estimate()).usage
      if (usage === undefined || !Number.isFinite(usage) || usage < 0)
        throw new Error("Origin estimate unavailable")
      return usage
    }),
    Promise.resolve().then(() => imageCacheBytes(caches)),
    Promise.resolve().then(() => dataCacheBytes(caches)),
  ])
  return {
    total: total.status === "fulfilled" ? total.value : undefined,
    images: images.status === "fulfilled" ? images.value : undefined,
    data: data.status === "fulfilled" ? data.value : undefined,
  }
}

/** Only automatic app caches are rebuildable; preferences and recent cards live elsewhere. */
export async function clearManagedCaches(): Promise<void> {
  const names = (await caches.keys()).filter((name) => name === IMAGE_CACHE || isDataCache(name))
  const results = await Promise.allSettled(names.map((name) => caches.delete(name)))
  if (results.some((result) => result.status === "rejected"))
    throw new Error("Cache clearing incomplete")
  if ((await caches.keys()).some((name) => name === IMAGE_CACHE || isDataCache(name)))
    throw new Error("Cache clearing incomplete")
}
