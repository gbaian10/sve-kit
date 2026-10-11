// @vitest-environment node
import { describe, expect, it, vi } from "vitest"

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

  it("bounds concurrent writes from separate instances to two records per slot", async () => {
    const storage = memoryCache()
    const cache = new VerifiedCache(root, "kind", 1, storage, "build")
    await cache.write("jp", hash(1), bytes("old"))
    await cache.write("jp", hash(2), bytes("previous"))
    await Promise.all(
      [3, 4, 5, 6].map((n) =>
        new VerifiedCache(root, "kind", 1, storage, "build").write("jp", hash(n), bytes(String(n))),
      ),
    )
    expect(await records(storage)).toHaveLength(2)
    const retained = await Promise.all(
      [3, 4, 5, 6].map(async (n) => text(await cache.read("jp", hash(n)))),
    )
    expect(retained.filter((value) => value !== undefined)).toHaveLength(2)
    for (const [index, value] of retained.entries())
      if (value !== undefined) expect(value).toBe(String(index + 3))
  })

  it("retains a rewritten previous record when a new manifest arrives", async () => {
    const storage = memoryCache()
    const cache = new VerifiedCache(root, "kind", 1, storage, "build")
    for (const n of [1, 2, 1, 3]) await cache.write("jp", hash(n), bytes(String(n)))
    expect(text(await cache.read("jp", hash(1)))).toBe("1")
    expect(await cache.read("jp", hash(2))).toBeUndefined()
    expect(text(await cache.read("jp", hash(3)))).toBe("3")
  })

  it("does not evict another slot while its write is still completing", async () => {
    const storage = memoryCache()
    let release: () => void = () => undefined
    let notify: () => void = () => undefined
    const paused = new Promise<void>((resolve) => {
      release = resolve
    })
    const inserted = new Promise<void>((resolve) => {
      notify = resolve
    })
    const delayed = {
      keys: storage.keys.bind(storage),
      delete: storage.delete.bind(storage),
      open: async (name: string) => {
        const cache = await storage.open(name)
        return {
          match: cache.match.bind(cache),
          keys: cache.keys.bind(cache),
          delete: cache.delete.bind(cache),
          put: async (key: string, response: Response) => {
            await cache.put(key, response)
            if (key.includes("/en/")) {
              notify()
              await paused
            }
          },
        } as unknown as Cache
      },
    } as unknown as CacheStorage
    const en = new VerifiedCache(root, "kind", 1, delayed, "build")
    const pending = en.write("en", hash(1), bytes("en"))
    await inserted
    const jp = new VerifiedCache(root, "kind", 1, storage, "build")
    await jp.write("jp", hash(1), bytes("jp"))
    release()
    await pending
    expect(text(await en.read("en", hash(1)))).toBe("en")
    expect(text(await jp.read("jp", hash(1)))).toBe("jp")
  })
})

it("scans automatic caches once for a successful write", async () => {
  const storage = memoryCache()
  const scan = vi.spyOn(storage, "keys")
  await new VerifiedCache(root, "search-view", 1, storage, "build").write(
    "jp",
    hash(1),
    bytes("ok"),
  )
  expect(scan).toHaveBeenCalledTimes(1)
})

it("a quota retry reuses one inventory and still enforces the view budget and generations", async () => {
  const storage = memoryCache()
  const name = `sve-search-view:${encodeURIComponent(root)}`
  const cache = new VerifiedCache(root, "search-view", 1, storage, "build")
  const payload = new Uint8Array(1024 * 1024)
  await cache.write("old", hash(1), payload)
  await cache.write("previous", hash(2), payload)
  const namespace = await storage.open(name)
  const open = storage.open.bind(storage)
  vi.spyOn(storage, "open").mockImplementation((key) =>
    key === name ? Promise.resolve(namespace) : open(key),
  )
  vi.spyOn(namespace, "put").mockRejectedValueOnce(new Error("quota"))
  const scan = vi.spyOn(storage, "keys")
  const next = new VerifiedCache(root, "search-view", 1, storage, "build")
  await next.write("new", hash(3), payload)
  expect(scan).toHaveBeenCalledTimes(1)
  expect(await next.read("old", hash(1))).toBeUndefined()
  expect((await next.read("new", hash(3)))?.length).toBe(payload.length)
  const retained = await namespace.keys()
  expect(retained.length).toBeLessThanOrEqual(2)
  expect(
    await Promise.all(
      retained.map(async (key) => Number((await namespace.match(key))?.headers.get("x-sve-bytes"))),
    ),
  ).toEqual(expect.arrayContaining([payload.length]))
})
