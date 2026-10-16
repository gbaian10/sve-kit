const MIB = 1024 * 1024
export const VIEW_BUDGET = 2 * MIB
const ROOT_BUDGET = 64 * MIB
const ORIGIN_BUDGET = 128 * MIB

export function isDataCache(name: string): boolean {
  return /^sve-(search-|snapshot:|images-)/u.test(name)
}

/** Counts declared payload bytes without trimming or materializing decoded data. */
export async function dataCacheBytes(storage: CacheStorage): Promise<number> {
  let total = 0
  for (const name of (await storage.keys()).filter(isDataCache)) {
    const cache = await storage.open(name)
    for (const key of await cache.keys()) {
      const response = await cache.match(key)
      const size = response?.headers.get("x-sve-bytes")
      if (size == null || size.trim() === "" || !Number.isFinite(Number(size)) || Number(size) < 0)
        throw new Error("Data payload size unavailable")
      total += Number(size)
    }
  }
  return total
}

/** Automatic data caches share one budget; bitmap and user-managed offline stores have their own. */
export async function trimDataCaches(
  storage: CacheStorage,
  root: string,
  reserve = 0,
  quota = false,
): Promise<void> {
  const budget = await readDataCacheBudget(storage, root)
  await budget.trim(reserve, quota)
}

/** A quota retry reuses the same inventory before and after inserting its replacement. */
export async function readDataCacheBudget(storage: CacheStorage, root: string) {
  const encoded = encodeURIComponent(root)
  const names = (await storage.keys()).filter(isDataCache)
  const entries: {
    cache: Cache
    url: string
    bytes: number
    root: boolean
    view: boolean
    touched: number
    hash?: string
  }[] = []
  for (const name of names) {
    const cache = await storage.open(name)
    for (const key of await cache.keys()) {
      const response = await cache.match(key)
      if (!response) continue
      const size = response.headers.get("x-sve-bytes")
      // Legacy automatic records can be rebuilt; never materialize their unbounded bodies to count.
      if (size === null || !Number.isFinite(Number(size)) || Number(size) < 0) {
        await cache.delete(key)
        continue
      }
      const hash = key.url.match(/\/sha256%3A([a-f0-9]{64})$/u)?.[1]
      entries.push({
        cache,
        url: key.url,
        bytes: Number(size),
        root: name.endsWith(`:${encoded}`) || name.startsWith(`sve-images-${encoded}:-`),
        view: name.startsWith("sve-search-view:"),
        touched: Number(response.headers.get("x-sve-touched") ?? 0),
        ...(hash ? { hash } : {}),
      })
    }
  }
  const trim = async (reserve = 0, quota = false) => {
    entries.sort((a, b) => a.touched - b.touched)
    const hashes = [
      ...new Set(
        [...entries]
          .reverse()
          .filter((e) => e.root && e.hash)
          .map((e) => e.hash),
      ),
    ].slice(0, 2)
    for (let i = entries.length - 1; i >= 0; i -= 1) {
      const e = entries[i]
      if (e?.root && e.hash && !hashes.includes(e.hash)) {
        await e.cache.delete(e.url)
        entries.splice(i, 1)
      }
    }
    const latestView = [...entries].reverse().find((e) => e.root && e.view)
    const pinned = (entry: (typeof entries)[number]) =>
      entry === latestView || (entry.root && entry.hash === hashes[0] && !entry.view)
    let total = entries.reduce((n, e) => n + e.bytes, reserve)
    let own = entries.filter((e) => e.root).reduce((n, e) => n + e.bytes, reserve)
    let views = entries.filter((e) => e.root && e.view).reduce((n, e) => n + e.bytes, 0)
    let reclaimed = 0
    const eviction = [...entries].sort(
      (a, b) => Number(pinned(a)) - Number(pinned(b)) || a.touched - b.touched,
    )
    for (const e of eviction) {
      // An active generation that cannot fit must fall back to misses rather than exceed the budget.
      if (pinned(e) && total <= ORIGIN_BUDGET && own <= ROOT_BUDGET && views <= VIEW_BUDGET)
        continue
      if (
        (!quota || reclaimed >= reserve) &&
        total <= ORIGIN_BUDGET &&
        own <= ROOT_BUDGET &&
        views <= VIEW_BUDGET
      )
        break
      if (
        (!quota || reclaimed >= reserve) &&
        total <= ORIGIN_BUDGET &&
        (!e.root || (own <= ROOT_BUDGET && (!e.view || views <= VIEW_BUDGET)))
      )
        continue
      await e.cache.delete(e.url)
      entries.splice(entries.indexOf(e), 1)
      reclaimed += e.bytes
      total -= e.bytes
      if (e.root) own -= e.bytes
      if (e.root && e.view) views -= e.bytes
    }
  }
  return {
    trim,
    record: (name: string, cache: Cache, url: string, bytes: number, touched: number) => {
      const hash = url.match(/\/sha256%3A([a-f0-9]{64})$/u)?.[1]
      entries.push({
        cache,
        url,
        bytes,
        touched,
        root: name.endsWith(`:${encoded}`) || name.startsWith(`sve-images-${encoded}:-`),
        view: name.startsWith("sve-search-view:"),
        ...(hash ? { hash } : {}),
      })
    },
  }
}
export function bodyHeaders(bytes: number): Record<string, string> {
  return { "x-sve-bytes": String(bytes), "x-sve-touched": String(Date.now()) }
}
