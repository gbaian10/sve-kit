import { BUILD_ID } from "./build-id"
import { transferDigest } from "./integrity"

// Browser storage can be cleared or damaged; a record is used only when its bytes still match.
const DIGEST = "x-sve-sha256"

/**
 * Results derived from a fully verified manifest, kept per root so reloading the same input skips
 * decoding and validation. Callers write only after every check passed; each slot keeps its current
 * and previous record. Unavailable storage, quota and unreadable records all mean a full rebuild.
 */
export class VerifiedCache {
  private readonly prefix: string
  private readonly version: string
  private readonly cache: Promise<Cache | undefined>

  constructor(
    root: string,
    kind: string,
    format: number,
    storage: CacheStorage | undefined,
    build = BUILD_ID,
  ) {
    this.prefix = `${root}/__sve-verified/${kind}`
    this.version = `v${String(format)}/${encodeURIComponent(build)}`
    this.cache = storage
      ? storage.open(`sve-${kind}:${encodeURIComponent(root)}`).then(
          (cache) => cache,
          () => undefined,
        )
      : Promise.resolve(undefined)
  }

  // Root, kind, format, build, slot and manifest are all in the URL: any change misses.
  private key(slot: string, manifestHash: string): string {
    return new Request(
      `${this.prefix}/${this.version}/${encodeURIComponent(slot)}/${encodeURIComponent(manifestHash)}`,
    ).url
  }

  private plan(slot: string): string {
    return new Request(`${this.prefix}/plans/${encodeURIComponent(slot)}`).url
  }

  async read(slot: string, manifestHash: string): Promise<Uint8Array | undefined> {
    const cache = await this.cache
    if (!cache) return undefined
    try {
      const key = this.key(slot, manifestHash)
      const response = await cache.match(key)
      if (!response) return undefined
      const payload = new Uint8Array(await response.arrayBuffer())
      if (response.headers.get(DIGEST) === (await transferDigest(payload))) return payload
      await cache.delete(key)
    } catch {
      // An unreadable record is a miss; the caller rebuilds from verified input.
    }
    return undefined
  }

  async write(slot: string, manifestHash: string, payload: Uint8Array<ArrayBuffer>): Promise<void> {
    const cache = await this.cache
    if (!cache) return
    try {
      const key = this.key(slot, manifestHash)
      const own = this.plan(slot)
      const plans = this.plan("")
      const readPlan = async (url: string): Promise<string[]> => {
        const value: unknown = await (await cache.match(url))?.json()
        return Array.isArray(value)
          ? value.filter((item): item is string => typeof item === "string")
          : []
      }
      // Writing the current record again keeps the previous distinct one.
      const current = [key, ...(await readPlan(own)).filter((item) => item !== key)].slice(0, 2)
      const urls = (await cache.keys()).map((request) => request.url)
      const retained = new Set([...current, own])
      for (const url of urls.filter((url) => url.startsWith(plans) && url !== own)) {
        retained.add(url)
        for (const item of await readPlan(url)) retained.add(item)
      }
      // Evict first so a full cache has room for the new record.
      for (const url of urls) if (!retained.has(url)) await cache.delete(url)
      const digest = await transferDigest(payload)
      await cache.put(key, new Response(payload, { headers: { [DIGEST]: digest } }))
      await cache.put(own, new Response(JSON.stringify(current)))
    } catch {
      // Quota or storage failures only cost a rebuild on the next load.
    }
  }
}
