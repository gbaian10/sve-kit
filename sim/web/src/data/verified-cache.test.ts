// @vitest-environment node
import { describe, expect, it } from "vitest"

import { memoryCache } from "../test-utils/cache"
import { VerifiedCache } from "./verified-cache"

const root = "https://cdn.test"
const hash = (n: number) => `sha256:${String(n).repeat(64).slice(0, 64)}`
const bytes = (text: string) => new TextEncoder().encode(text)
const text = (value: Uint8Array | undefined) =>
  value === undefined ? undefined : new TextDecoder().decode(value)
const records = async (storage: CacheStorage, kind = "kind", base = root) =>
  (await (await storage.open(`sve-${kind}:${encodeURIComponent(base)}`)).keys())
    .map((request) => request.url)
    .filter((url) => !url.includes("/plans/"))

describe("VerifiedCache", () => {
  it("reads back only the exact root, kind, format, build, slot and manifest it wrote", async () => {
    const storage = memoryCache()
    await new VerifiedCache(root, "kind", 1, storage, "build-a").write("jp", hash(1), bytes("ok"))
    const read = (cache: VerifiedCache, slot = "jp", manifest = hash(1)) =>
      cache.read(slot, manifest).then(text)
    expect(await read(new VerifiedCache(root, "kind", 1, storage, "build-a"))).toBe("ok")
    for (const other of [
      new VerifiedCache(`${root}/other`, "kind", 1, storage, "build-a"),
      new VerifiedCache(root, "other", 1, storage, "build-a"),
      new VerifiedCache(root, "kind", 2, storage, "build-a"),
      new VerifiedCache(root, "kind", 1, storage, "build-b"),
    ])
      expect(await read(other)).toBeUndefined()
    const same = new VerifiedCache(root, "kind", 1, storage, "build-a")
    expect(await read(same, "en")).toBeUndefined()
    expect(await read(same, "jp", hash(2))).toBeUndefined()
  })

  it("treats a damaged record as a miss and removes it", async () => {
    const storage = memoryCache()
    const cache = new VerifiedCache(root, "kind", 1, storage, "build")
    await cache.write("jp", hash(1), bytes("original"))
    const [url = ""] = await records(storage)
    const namespace = await storage.open(`sve-kind:${encodeURIComponent(root)}`)
    const response = await namespace.match(url)
    if (!response) throw new Error("missing record")
    await namespace.put(url, new Response("tampered", { headers: response.headers }))
    expect(await cache.read("jp", hash(1))).toBeUndefined()
    expect(await records(storage)).toEqual([])
  })

  it("keeps the current and previous record per slot, leaving other slots and roots alone", async () => {
    const storage = memoryCache()
    const cache = new VerifiedCache(root, "kind", 1, storage, "build")
    const other = new VerifiedCache(`${root}/other`, "kind", 1, storage, "build")
    await other.write("jp", hash(1), bytes("other root"))
    await cache.write("en", hash(1), bytes("en"))
    for (const n of [1, 2, 3]) await cache.write("jp", hash(n), bytes(`jp ${String(n)}`))
    // Writing the current record again must not push out the previous distinct one.
    await cache.write("jp", hash(3), bytes("jp 3"))
    expect(await cache.read("jp", hash(1))).toBeUndefined()
    expect(text(await cache.read("jp", hash(2)))).toBe("jp 2")
    expect(text(await cache.read("jp", hash(3)))).toBe("jp 3")
    expect(text(await cache.read("en", hash(1)))).toBe("en")
    expect(await records(storage)).toHaveLength(3)
    expect(text(await other.read("jp", hash(1)))).toBe("other root")
    // A newer build replaces the oldest record of its slot like any other new input.
    await new VerifiedCache(root, "kind", 1, storage, "next").write("jp", hash(3), bytes("next"))
    expect(await cache.read("jp", hash(2))).toBeUndefined()
    expect(text(await cache.read("jp", hash(3)))).toBe("jp 3")
  })

  it("reports a miss, never an error, when storage is unavailable or full", async () => {
    const rejecting = {
      open: () => Promise.reject(new Error("blocked")),
    } as unknown as CacheStorage
    for (const storage of [undefined, rejecting, memoryCache(true)]) {
      const cache = new VerifiedCache(root, "kind", 1, storage, "build")
      await expect(cache.write("jp", hash(1), bytes("data"))).resolves.toBeUndefined()
      expect(await cache.read("jp", hash(1))).toBeUndefined()
    }
    // A relative root cannot form a URL outside a page; that is unavailable storage too.
    const relative = new VerifiedCache("/cdn", "kind", 1, memoryCache(), "build")
    await relative.write("jp", hash(1), bytes("data"))
    expect(await relative.read("jp", hash(1))).toBeUndefined()
  })
})
