const MIB = 1024 * 1024
export const VIEW_BUDGET = 2 * MIB
const ROOT_BUDGET = 64 * MIB
const ORIGIN_BUDGET = 128 * MIB

/** Automatic data caches share one budget; bitmap and user-managed offline stores have their own. */
export async function trimDataCaches(
  storage: CacheStorage,
  root: string,
  reserve = 0,
  quota = false,
): Promise<void> {
  const encoded = encodeURIComponent(root)
  const names = (await storage.keys()).filter((name) =>
    /^sve-(search-|snapshot:|images-)/u.test(name),
  )
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
      const hash = key.url.match(/\/sha256%3A([a-f0-9]{64})$/u)?.[1]
      entries.push({
        cache,
        url: key.url,
        bytes: size === null ? (await response.arrayBuffer()).byteLength : Number(size),
        root: name.endsWith(`:${encoded}`) || name.startsWith(`sve-images-${encoded}:-`),
        view: name.startsWith("sve-search-view:"),
        touched: Number(response.headers.get("x-sve-touched") ?? 0),
        ...(hash ? { hash } : {}),
      })
    }
  }
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
    if (pinned(e) && total <= ORIGIN_BUDGET && own <= ROOT_BUDGET && views <= VIEW_BUDGET) continue
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
    reclaimed += e.bytes
    total -= e.bytes
    if (e.root) own -= e.bytes
    if (e.root && e.view) views -= e.bytes
  }
}
export function bodyHeaders(bytes: number): Record<string, string> {
  return { "x-sve-bytes": String(bytes), "x-sve-touched": String(Date.now()) }
}
