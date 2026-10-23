import type { Fetcher } from "../cdn"
import type { JsonObject } from "../format-v3/json"
import { rawBlobDescriptor, RawBlobStore, rawRetentionPlan } from "../raw-blobs"

/** Search owns migration and whole-manifest plans; raw bytes belong to the shared store. */
export class SearchBlobs {
  private readonly raw: RawBlobStore
  private readonly migration: Promise<void>

  constructor(base: string, fetcher: Fetcher, storage?: CacheStorage) {
    this.raw = new RawBlobStore(base, fetcher, storage)
    this.migration = (async () => {
      try {
        await storage?.delete(`sve-search-blobs:${encodeURIComponent(base)}`)
      } catch {
        // Denied storage must not block ordinary on-demand reads; a later session retries.
      }
    })()
  }

  get persistent(): boolean {
    return this.raw.persistent
  }

  async retainPlan(files: ReadonlyMap<string, JsonObject>, manifestHash: string) {
    await this.migration
    await this.raw.retain(files, manifestHash)
    return rawRetentionPlan([{ hash: manifestHash, files }])
  }

  async read(file: JsonObject, signal: AbortSignal): Promise<Uint8Array> {
    await this.migration
    return this.raw.read(rawBlobDescriptor(file), { signal })
  }
}
