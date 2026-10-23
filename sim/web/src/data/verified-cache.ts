import { BUILD_ID } from "./build-id"
import { bodyHeaders, readDataCacheBudget, trimDataCaches } from "./cache-budget"
import { transferDigest } from "./integrity"

// Browser storage can be cleared or damaged; a record is used only when its bytes still match.
const DIGEST = "x-sve-sha256"

// Store only successful decoding results; damaged or unreadable records require a full rebuild.
// Root and build isolation lets reloads reuse the current result without repeating its decoding.
export class VerifiedCache {
  private readonly root: string
  private readonly storage: CacheStorage | undefined
  lastRead = { readMs: 0, digestMs: 0 }
  private readonly prefix: string
  private readonly version: string
  private readonly name: string
  private readonly cache: Promise<Cache | undefined>

  constructor(
    root: string,
    kind: string,
    format: number,
    storage: CacheStorage | undefined,
    build = BUILD_ID,
  ) {
    this.root = root
    this.name = `sve-${kind}:${encodeURIComponent(root)}`
    this.storage = storage
    this.prefix = `${root}/__sve-verified/${kind}`
    this.version = `v${String(format)}/${encodeURIComponent(build)}`
    this.cache = storage
      ? storage.open(this.name).then(
          (cache) => cache,
          () => undefined,
        )
      : Promise.resolve(undefined)
  }

  private slotPrefix(slot: string): string {
    return new Request(`${this.prefix}/${encodeURIComponent(slot)}/`).url
  }

  // Root, kind, format, build, slot and manifest are all in the URL: any change misses.
  private key(slot: string, manifestHash: string): string {
    return new Request(
      `${this.slotPrefix(slot)}${this.version}/${encodeURIComponent(manifestHash)}`,
    ).url
  }

  async remove(slot: string, manifestHash: string): Promise<void> {
    const cache = await this.cache
    if (!cache) return
    try {
      await cache.delete(this.key(slot, manifestHash))
    } catch {
      // Removing an unusable record is best effort when storage fails.
    }
  }

  async read(slot: string, manifestHash: string): Promise<Uint8Array | undefined> {
    const start = performance.now()
    const cache = await this.cache
    if (!cache) return undefined
    try {
      const key = this.key(slot, manifestHash)
      const response = await cache.match(key)
      if (!response) return undefined
      const payload = new Uint8Array(await response.arrayBuffer())
      const mark = performance.now()
      const digest = await transferDigest(payload)
      this.lastRead = { readMs: mark - start, digestMs: performance.now() - mark }
      if (response.headers.get(DIGEST) === digest) return payload
      await this.remove(slot, manifestHash)
    } catch {
      // An unreadable record is a miss; the caller rebuilds from verified input.
      await this.remove(slot, manifestHash)
    }
    return undefined
  }

  async write(slot: string, manifestHash: string, payload: Uint8Array<ArrayBuffer>): Promise<void> {
    const cache = await this.cache
    if (!cache) return
    try {
      const key = this.key(slot, manifestHash)
      const prefix = this.slotPrefix(slot)
      const ownKeys = async () => {
        const keys = (await cache.keys()).filter((request) => request.url.startsWith(prefix))
        const dated = await Promise.all(
          keys.map(async (request) => ({
            url: request.url,
            touched: Number((await cache.match(request))?.headers.get("x-sve-touched") ?? 0),
          })),
        )
        return dated.sort((a, b) => a.touched - b.touched).map(({ url }) => url)
      }
      const prune = async (urls: string[], keep: number) => {
        for (const url of urls.slice(0, Math.max(0, urls.length - keep))) await cache.delete(url)
      }
      const digest = await transferDigest(payload)
      const response = () =>
        new Response(payload, { headers: { [DIGEST]: digest, ...bodyHeaders(payload.length) } })
      let retried = false
      try {
        await cache.put(key, response())
      } catch {
        if (!this.storage) return
        const budget = await readDataCacheBudget(this.storage, this.root)
        if (!(await budget.trim(payload.length, true))) return
        const replacement = response()
        await cache.put(key, replacement)
        budget.record(
          this.name,
          cache,
          key,
          payload.length,
          Number(replacement.headers.get("x-sve-touched")),
        )
        if (!(await budget.trim())) await cache.delete(key)
        retried = true
      }
      if (this.storage && !retried && !(await trimDataCaches(this.storage, this.root)))
        await cache.delete(key)
      // Sweep after insertion too: concurrent writers cannot leave orphaned records or grow a slot.
      const keys = await ownKeys()
      await prune([...keys.filter((url) => url !== key), ...keys.filter((url) => url === key)], 2)
    } catch {
      // Quota or storage failures only cost a rebuild on the next load.
    }
  }
}
