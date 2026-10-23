import { retainedReferences } from "./retained-manifests"

const MIB = 1024 * 1024
export const VIEW_BUDGET = 2 * MIB
const ROOT_BUDGET = 96 * MIB
const ORIGIN_BUDGET = 128 * MIB

// The adopted-selection pointer names a retained manifest; losing it would drop the active choice.
const activeSelection = (url: string) => /\/active$/u.test(url)

export function isDataCache(name: string): boolean {
  return /^sve-(search-|snapshot:|images-|raw:|retained:)/u.test(name)
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
  collect = false,
): Promise<boolean> {
  const budget = await readDataCacheBudget(storage, root)
  return budget.trim(reserve, quota, collect)
}

/** A quota retry reuses the same inventory before and after inserting its replacement. */
export async function readDataCacheBudget(storage: CacheStorage, root: string) {
  const encoded = encodeURIComponent(root)
  const names = (await storage.keys()).filter(isDataCache)
  const retained = await retainedReferences(storage, names)
  const entries: {
    cache: Cache
    url: string
    bytes: number
    root: boolean
    view: boolean
    touched: number
    hash?: string
    rawHash?: string
    protected: boolean
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
      const rawHash = key.url.match(/\/__sve-raw\/(sha256%3A[a-f0-9]{64})\/bytes$/u)?.[1]
      const refs = retained.get(name.slice(name.lastIndexOf(":") + 1))
      entries.push({
        cache,
        url: key.url,
        bytes: Number(size),
        root: name.endsWith(`:${encoded}`) || name.startsWith(`sve-images-${encoded}:-`),
        view: name.startsWith("sve-search-view:"),
        touched: Number(response.headers.get("x-sve-touched") ?? 0),
        ...(hash ? { hash } : {}),
        ...(rawHash ? { rawHash: decodeURIComponent(rawHash) } : {}),
        protected:
          name.startsWith("sve-retained:") ||
          !!(
            refs &&
            (rawHash
              ? refs.raw.has(decodeURIComponent(rawHash))
              : hash
                ? !name.startsWith("sve-search-view:") && refs.manifests.has(`sha256:${hash}`)
                : activeSelection(key.url))
          ),
      })
    }
  }
  const trim = async (reserve = 0, quota = false, collect = false) => {
    let reclaimed = 0
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
      const refs = retained.get(encoded)
      if (
        e?.root &&
        !e.protected &&
        ((e.hash &&
          (!refs || collect) &&
          !(refs ? refs.manifests.has(`sha256:${e.hash}`) : hashes.includes(e.hash))) ||
          (collect && refs && e.rawHash && !refs.raw.has(e.rawHash)))
      ) {
        await e.cache.delete(e.url)
        reclaimed += e.bytes
        entries.splice(i, 1)
      }
    }
    const latestView = [...entries].reverse().find((e) => e.root && e.view)
    const pinned = (entry: (typeof entries)[number]) =>
      entry.protected ||
      entry === latestView ||
      (entry.root && entry.hash === hashes[0] && !entry.view)
    let total = entries.reduce((n, e) => n + e.bytes, reserve)
    let own = entries.filter((e) => e.root).reduce((n, e) => n + e.bytes, reserve)
    let views = entries.filter((e) => e.root && e.view).reduce((n, e) => n + e.bytes, 0)
    const eviction = [...entries].sort(
      (a, b) => Number(pinned(a)) - Number(pinned(b)) || a.touched - b.touched,
    )
    for (const e of eviction) {
      if (e.protected) continue
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
    return (
      total <= ORIGIN_BUDGET &&
      own <= ROOT_BUDGET &&
      views <= VIEW_BUDGET &&
      (!quota || reclaimed >= reserve)
    )
  }
  return {
    fits: (blobs: readonly { readonly sha256: string; readonly bytes: number }[]) => {
      const raw = new Map(blobs.map((blob) => [blob.sha256, blob.bytes]))
      let own = 0
      let total = 0
      for (const entry of entries) {
        // Keep derived data and the previous active bytes; overlapping raw hashes count once.
        const hash = entry.root
          ? entry.url.match(/\/__sve-raw\/(sha256%3A[a-f0-9]{64})\/bytes$/u)?.[1]
          : undefined
        if (hash && raw.has(decodeURIComponent(hash))) continue
        total += entry.bytes
        if (entry.root) own += entry.bytes
      }
      const reserve = [...raw.values()].reduce((sum, bytes) => sum + bytes, 0)
      return own + reserve <= ROOT_BUDGET && total + reserve <= ORIGIN_BUDGET
    },
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
        protected: !!(
          hash &&
          !name.startsWith("sve-search-view:") &&
          retained.get(name.slice(name.lastIndexOf(":") + 1))?.manifests.has(`sha256:${hash}`)
        ),
      })
    },
  }
}
let lastTouched = 0
export function bodyHeaders(bytes: number): Record<string, string> {
  lastTouched = Math.max(Date.now(), lastTouched + 1)
  return { "x-sve-bytes": String(bytes), "x-sve-touched": String(lastTouched) }
}
