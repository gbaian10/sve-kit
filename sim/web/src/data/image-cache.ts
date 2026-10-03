const CACHE = "sve-image-responses-v2"
const MAX_IMAGES = 128

export function versionedImage(url: string): boolean {
  const value = new URL(url)
  return (
    /^.*\/images\/(card_[sml]|art_[sm])\/[1-9][0-9]*(?:-f[1-9][0-9]*)?\.webp$/.test(
      value.pathname,
    ) && /^\?v=[1-9][0-9]*$/.test(value.search)
  )
}

/** HTTP/CDN freshness is a publisher obligation; this cache never aliases versions or paths. */
export async function cachedImage(
  request: Request,
  storage: CacheStorage,
  network: typeof fetch,
): Promise<Response> {
  let cache: Cache | undefined
  try {
    cache = await storage.open(CACHE)
    const hit = await cache.match(request)
    if (hit?.type === "opaque") await cache.delete(request)
    else if (hit) return hit
  } catch {
    cache = undefined
  }
  const response = await network(request)
  // Opaque entries can consume padded quota far beyond the image's actual size.
  if (cache && response.type !== "opaque" && response.ok) {
    try {
      await cache.put(request, response.clone())
      const keys = await cache.keys()
      for (const key of keys.slice(0, Math.max(0, keys.length - MAX_IMAGES)))
        await cache.delete(key)
    } catch {
      // Storage is optional; failures must not replace a valid network response with an old URL.
    }
  }
  return response
}
