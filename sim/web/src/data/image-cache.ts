export const IMAGE_CACHE = "sve-image-responses-v2"

export function versionedImage(url: string): boolean {
  const value = new URL(url)
  return (
    /^.*\/images\/(card_[sl]|art_s)\/[1-9][0-9]*(?:-f[1-9][0-9]*)?\.webp$/.test(value.pathname) &&
    /^\?v=[1-9][0-9]*$/.test(value.search)
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
    cache = await storage.open(IMAGE_CACHE)
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
      const version = BigInt(new URL(request.url).searchParams.get("v") ?? "0")
      const keys = await cache.keys(request, { ignoreSearch: true })
      for (const key of keys) {
        if (!versionedImage(key.url)) continue
        const older = BigInt(new URL(key.url).searchParams.get("v") ?? "0")
        if (older < version) await cache.delete(key)
      }
    } catch {
      // Storage is optional; failures must not replace a valid network response with an old URL.
    }
  }
  return response
}
