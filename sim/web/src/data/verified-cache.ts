import { BUILD_ID } from "./build-id"
import { transferDigest } from "./integrity"

// Browser storage can be cleared or damaged; a record is used only when its bytes still match.
const DIGEST = "x-sve-sha256"

// Store only successful decoding results; damaged or unreadable records require a full rebuild.
// Root and build isolation lets reloads reuse the current result without repeating its decoding.
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
    const cache = await this.cache
    if (!cache) return undefined
    try {
      const key = this.key(slot, manifestHash)
      const response = await cache.match(key)
      if (!response) return undefined
      const payload = new Uint8Array(await response.arrayBuffer())
      if (response.headers.get(DIGEST) === (await transferDigest(payload))) return payload
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
      const ownKeys = async () =>
        (await cache.keys()).map((request) => request.url).filter((url) => url.startsWith(prefix))
      const prune = async (urls: string[], keep: number) => {
        for (const url of urls.slice(0, Math.max(0, urls.length - keep))) await cache.delete(url)
      }
      const before = await ownKeys()
      // Cache keys are in insertion order. Rewrites retain the previous distinct record.
      await prune(before, before.includes(key) ? 2 : 1)
      const digest = await transferDigest(payload)
      // Reinsert a reused record so it remains the current one in insertion order.
      await cache.delete(key)
      await cache.put(key, new Response(payload, { headers: { [DIGEST]: digest } }))
      // Sweep after insertion too: concurrent writers cannot leave orphaned records or grow a slot.
      await prune(await ownKeys(), 2)
    } catch {
      // Quota or storage failures only cost a rebuild on the next load.
    }
  }
}
