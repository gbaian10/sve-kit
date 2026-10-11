import type { Region } from "../../domain/search"
import { bodyHeaders, trimDataCaches } from "../cache-budget"
import { fetchBytes, type Fetcher } from "../cdn"
import { SnapshotError } from "../format-v3/errors"
import { integerValue, type JsonObject, stringValue } from "../format-v3/json"
import { transferDigest } from "../integrity"

/** Hash paths share persistent content across data versions; RAM owns only the current file. */
export class SearchBlobs {
  private readonly storage: CacheStorage | undefined
  private readonly base: string
  private readonly fetcher: Fetcher
  private readonly cache: Promise<Cache | undefined>
  persistent = false

  constructor(base: string, fetcher: Fetcher, storage?: CacheStorage) {
    this.storage = storage
    this.base = base
    this.fetcher = fetcher
    this.cache = storage
      ? storage.open(`sve-search-blobs:${encodeURIComponent(base)}`).then(
          (cache) => {
            this.persistent = true
            return cache
          },
          () => undefined,
        )
      : Promise.resolve(undefined)
  }

  async retainPlan(
    edition: Region,
    files: readonly JsonObject[],
    manifestHash?: string,
  ): Promise<void> {
    const cache = await this.cache
    if (!cache) return
    const planPath = (region: Region) => new Request(`${this.base}/__sve-search-plan/${region}`).url
    try {
      const writePlan = async (path: string, plans: unknown) => {
        const bytes = new TextEncoder().encode(JSON.stringify(plans))
        await cache.put(path, new Response(bytes, { headers: bodyHeaders(bytes.length) }))
      }
      const current = [
        ...new Set(
          files.map((file) => new Request(`${this.base}/${stringValue(file["path"])}`).url),
        ),
      ].sort()
      const readPlan = async (region: Region): Promise<string[][]> => {
        const response = await cache.match(planPath(region))
        return response ? ((await response.json()) as string[][]) : []
      }
      if (manifestHash) {
        const generations = new Request(`${this.base}/__sve-search-plan/generations`).url
        const response = await cache.match(generations)
        const previous = response
          ? ((await response.json()) as { hash: string; paths: string[] }[])
          : []
        const plans = [
          { hash: manifestHash, paths: current },
          ...previous.filter((plan) => plan.hash !== manifestHash),
        ].slice(0, 2)
        const retained = new Set([generations, ...plans.flatMap((plan) => plan.paths)])
        for (const key of await cache.keys()) if (!retained.has(key.url)) await cache.delete(key)
        await writePlan(generations, plans)
        if (this.storage) await trimDataCaches(this.storage, this.base)
        return
      }
      const previous = await readPlan(edition)
      const plans =
        JSON.stringify(previous[0]) === JSON.stringify(current)
          ? previous
          : [current, ...previous].slice(0, 2)
      const other = await readPlan(edition === "jp" ? "en" : "jp")
      const retained = new Set([...plans.flat(), ...other.flat(), planPath("jp"), planPath("en")])
      // Evict before writing metadata so a full cache can recover space for the next load.
      for (const key of await cache.keys()) if (!retained.has(key.url)) await cache.delete(key)
      await writePlan(planPath(edition), plans)
      if (this.storage) await trimDataCaches(this.storage, this.base)
    } catch {
      this.persistent = false
    }
  }

  async read(file: JsonObject, signal: AbortSignal): Promise<Uint8Array> {
    const path = `${this.base}/${stringValue(file["path"])}`
    const expected = stringValue(file["sha256"])
    const valid = async (data: Uint8Array) =>
      data.length === integerValue(file["bytes"]) && (await transferDigest(data)) === expected
    const cache = await this.cache
    if (cache && this.persistent) {
      try {
        const response = await cache.match(path)
        if (response) {
          const data = new Uint8Array(await response.arrayBuffer())
          if (await valid(data)) {
            signal.throwIfAborted()
            return data
          }
          await cache.delete(path)
        }
      } catch {
        this.persistent = false
      }
    }
    for (let attempt = 0; attempt < 2; attempt += 1) {
      const data = await fetchBytes(this.fetcher, path, { signal })
      signal.throwIfAborted()
      if (!(await valid(data))) continue
      if (cache && this.persistent) {
        try {
          try {
            await cache.put(
              path,
              new Response(data.slice().buffer, { headers: bodyHeaders(data.length) }),
            )
          } catch {
            if (!this.storage) throw new Error("cache unavailable")
            await trimDataCaches(this.storage, this.base, data.length, true)
            await cache.put(
              path,
              new Response(data.slice().buffer, { headers: bodyHeaders(data.length) }),
            )
          }
        } catch {
          this.persistent = false
        }
      }
      return data
    }
    throw new SnapshotError("blob-integrity", "search blob hash/length mismatch after retry")
  }
}
