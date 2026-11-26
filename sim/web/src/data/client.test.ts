// @vitest-environment node
import { describe, expect, it } from "vitest"

import { buildSnapshot } from "../../scripts/fixture/build"
import { memoryCache } from "../test-utils/cache"
import { catalogOf } from "./catalog"
import { type Fetcher } from "./cdn"
import { createSnapshotClient } from "./client"
import {
  canonical,
  type JsonObject,
  type JsonValue,
  objectValue,
  parseJson,
  stringValue,
} from "./format-v3/json"
import { transferDigest } from "./integrity"
import { VerifiedCache } from "./verified-cache"

const stubImage = ({ width, height, seed }: { width: number; height: number; seed: number }) =>
  Promise.resolve(
    new TextEncoder().encode(`stub-webp:${String(width)}x${String(height)}:${String(seed)}`),
  )
const built = await buildSnapshot({ encodeImage: stubImage })

const keyFor = (table: string, partition: string, owner: string | null = null): string => {
  const file = (built.manifest["files"] as JsonObject[]).find((f) =>
    (f["row_counts"] as JsonValue[][]).some(
      (r) =>
        r[0] === table &&
        r[3] === partition &&
        r[2] === owner &&
        (f["region"] === "jp" || f["region"] === null),
    ),
  )
  if (!file) throw new Error(`fixture has no ${table}/${partition}`)
  return stringValue(file["key"])
}
const printingKey = keyFor("printing", "detail", "set:bp01")
const revisionKey = keyFor("face_revision", "detail", "set:bp01")
const textKey = keyFor("text_unit", "detail")

interface Served {
  readonly fetcher: Fetcher
  readonly requests: string[]
  readonly files: Map<string, Uint8Array>
  fail: (predicate: (path: string) => boolean) => void
}

/** Serves an in-memory snapshot root under `base`, recording every request. */
function serve(base: string, files = new Map(built.files)): Served {
  const requests: string[] = []
  let failing: (path: string) => boolean = () => false
  const fetcher: Fetcher = (url) => {
    if (!url.startsWith(`${base}/`)) return Promise.resolve(new Response(null, { status: 404 }))
    const path = url.slice(base.length + 1)
    requests.push(path)
    if (failing(path)) return Promise.reject(new TypeError("Failed to fetch"))
    const bytes = files.get(path)
    if (!bytes) return Promise.resolve(new Response(null, { status: 404 }))
    return Promise.resolve(new Response(bytes.slice().buffer, { status: 200 }))
  }
  return {
    fetcher,
    requests,
    files,
    fail: (predicate) => {
      failing = predicate
    },
  }
}

describe("createSnapshotClient", () => {
  it("reads a pinned match manifest without consulting or adopting the current selection", async () => {
    const served = serve("/cdn")
    const path = [...served.files.keys()].find((item) => item.startsWith("snapshots/manifests/"))
    const bytes = path === undefined ? undefined : served.files.get(path)
    if (path === undefined || bytes === undefined) throw new Error("missing fixture manifest")
    const hash = await transferDigest(bytes)
    const cacheStorage = memoryCache()
    served.fail((item) => item === "snapshots/versions/index.json")
    const client = createSnapshotClient("/cdn", {
      fetch: served.fetcher,
      cacheStorage,
      pinnedManifest: hash,
    })
    await client.load()
    expect(client.snapshot()?.manifestHash).toBe(hash)
    expect(served.requests).toContain(path)
    expect(served.requests).not.toContain("snapshots/versions/index.json")

    const current = createSnapshotClient("/cdn", { fetch: served.fetcher, cacheStorage })
    await current.load()
    expect(current.snapshot()).toBeNull()
    expect(current.status()).toMatchObject({ state: "error", kind: "network" })
  })

  it("retains finite regional rows across same-version switches without duplicating background printings", async () => {
    const client = createSnapshotClient("/cdn", {
      fetch: serve("/cdn").fetcher,
      cacheStorage: memoryCache(),
    })
    await client.load()
    await client.prefetchPrintings?.()
    await client.selectRegion?.("en")
    await client.prefetchPrintings?.()
    await client.selectRegion?.("jp")
    await client.prefetchPrintings?.()
    const snapshot = client.snapshot()
    if (!snapshot) throw new Error("missing switched snapshot")
    const index = catalogOf(snapshot).index
    expect(index.cards.some((card) => (card["regions"] as JsonObject[]).length === 2)).toBe(true)
    const printings = index.cards.flatMap((card) => index.printingsOf(stringValue(card["id"])))
    expect(new Set(printings.map((row) => row["id"])).size).toBe(printings.length)
    expect(new Set(printings.map((row) => row["region"]))).toEqual(new Set(["jp", "en"]))
  })

  it.each([
    { format: 3, kind: "incompatible" },
    { format: 1, kind: "corrupt" },
    { format: 0, kind: "corrupt" },
    { format: -1, kind: "corrupt" },
    { format: 2.5, kind: "corrupt" },
    { format: 3.5, kind: "corrupt" },
    { format: "3", kind: "corrupt" },
    { format: null, kind: "corrupt" },
    { format: true, kind: "corrupt" },
    { format: undefined, kind: "corrupt" },
  ])("classifies index format $format as $kind", async ({ format, kind }) => {
    const served = serve("/cdn")
    const path = "snapshots/versions/index.json"
    const index = objectValue(parseJson(served.files.get(path) ?? ""))
    served.files.set(
      path,
      new TextEncoder().encode(JSON.stringify({ ...index, index_format: format })),
    )
    const client = createSnapshotClient("/cdn", { fetch: served.fetcher })
    await client.load()
    expect(client.status()).toMatchObject({ state: "error", kind })
    expect(client.snapshot()).toBeNull()
  })

  it.each(["{", "{}"])(
    "reports corrupt data when hash-verified manifest JSON cannot be read (%s)",
    async (text) => {
      const served = serve("/cdn")
      const bytes = new TextEncoder().encode(text)
      const hash = await transferDigest(bytes)
      const path = `snapshots/manifests/${hash.slice(7)}.json`
      served.files.set(path, bytes)
      const index = objectValue(parseJson(served.files.get("snapshots/versions/index.json") ?? ""))
      const current = objectValue(index["current"])
      served.files.set(
        "snapshots/versions/index.json",
        canonical({
          ...index,
          current: { ...current, manifest_path: path, manifest_sha256: hash },
        }),
      )
      const client = createSnapshotClient("/cdn", { fetch: served.fetcher })
      await client.load()
      expect(client.status()).toMatchObject({ state: "error", kind: "corrupt" })
      expect(client.snapshot()).toBeNull()
    },
  )

  it("loads detail files on demand, joins them onto the bootstrap and keeps an LRU", async () => {
    const served = serve("/cdn")
    const client = createSnapshotClient("/cdn", {
      fetch: served.fetcher,
      detailCacheSize: 1,
      cacheStorage: memoryCache(),
    })
    await client.load()
    const bp01 = await client.fragments(printingKey)
    const printing = bp01.find((fragment) => fragment.table === "printing")
    expect(printing?.rows[0]?.["id"]).toBeTypeOf("string")
    expect((printing?.rows[0]?.["faces"] as JsonObject[])[0]?.["printed_text_state"]).toBeTypeOf(
      "string",
    )
    const revision = (await client.fragments(revisionKey)).find(
      (fragment) => fragment.table === "face_revision",
    )
    expect(revision?.rows[0]?.["effect_unit_id"]).toBeTypeOf("string")
    await client.fragments(printingKey)
    const before = served.requests.length
    await client.fragments(printingKey)
    expect(served.requests.length).toBe(before)
    await client.fragments(textKey)
    const afterText = served.requests.length
    await client.fragments(printingKey)
    expect(afterText).toBe(before)
    expect(served.requests.length).toBe(afterText)
    await expect(client.fragments("programs")).rejects.toThrow("not a detail file")
    await expect(client.fragments("nope")).rejects.toThrow("unknown file")
  })
})

describe("createSnapshotClient: verified snapshot records", () => {
  const base = "https://cdn.test"
  const INDEX = "snapshots/versions/index.json"
  const namespace = (storage: CacheStorage) =>
    storage.open(`sve-snapshot:${encodeURIComponent(base)}`)
  const records = async (storage: CacheStorage) =>
    (await (await namespace(storage)).keys()).filter(
      (request) => !request.url.includes("/plans/") && !request.url.includes("/adopted-"),
    )
  // The record is written after the page had a chance to render, in short slices.
  const persisted = (storage: CacheStorage) =>
    expect
      .poll(
        async () => {
          const cache = await namespace(storage)
          return (await cache.keys()).some((request) => request.url.includes("/adopted-index-jp/"))
        },
        { timeout: 10_000 },
      )
      .toBe(true)
  const phases = (client: ReturnType<typeof createSnapshotClient>) => {
    const seen: string[] = []
    client.subscribe(() => {
      const status = client.status()
      seen.push(status.state === "loading" ? status.phase : status.state)
    })
    return seen
  }

  it.each(["adoption", "notification", "repeated notification"])(
    "recovers from a cached startup %s exception",
    async (failure) => {
      const storage = memoryCache()
      const original = createSnapshotClient(base, {
        fetch: serve(base).fetcher,
        cacheStorage: storage,
      })
      await original.load()
      await persisted(storage)
      const cache = await namespace(storage)
      const [key] = await records(storage)
      const response = key && (await cache.match(key))
      const snapshot = original.snapshot()
      if (!response || !snapshot) throw new Error("missing stored snapshot")
      const originalBytes = new Uint8Array(await response.arrayBuffer())
      const stored = new VerifiedCache(base, "snapshot", 2, storage)
      if (failure === "adoption") {
        const record = objectValue(parseJson(originalBytes))
        const manifest = objectValue(record["manifest"])
        const files = manifest["files"] as JsonObject[]
        // This is readable by catalog restore but fails when constructing image metadata at adoption.
        await stored.write(
          "snapshot-jp",
          snapshot.manifestHash,
          canonical({
            ...record,
            manifest: {
              ...manifest,
              files: files.map((file) =>
                file["role"] === "images" ? { ...file, row_counts: null } : file,
              ),
            },
          }).slice(),
        )
      }
      const client = createSnapshotClient(base, {
        fetch: serve(base).fetcher,
        cacheStorage: storage,
      })
      let notified = false
      const unsubscribe = client.subscribe(() => {
        if ((failure === "notification" && !notified) || failure === "repeated notification") {
          notified = true
          throw new Error("startup notification failed")
        }
      })
      if (failure === "repeated notification")
        await expect(client.load()).rejects.toThrow("startup notification failed")
      else await expect(client.load()).resolves.toBeUndefined()
      expect(client.status()).toMatchObject({ state: "error", kind: "corrupt" })
      expect(client.snapshot()).toBeNull()
      unsubscribe()
      await stored.write("snapshot-jp", snapshot.manifestHash, originalBytes)
      await client.retry()
      await client.reload()
      expect(client.status()).toEqual(original.status())
      expect(client.snapshot()).toStrictEqual(snapshot)
    },
  )

  it.each([
    { revision: 3, detail: "backwards" },
    { revision: 2, detail: "changed without a new revision" },
  ])(
    "retry preserves the session's revision $revision despite another tab's older selection",
    async ({ revision, detail }) => {
      const storage = memoryCache()
      const original = createSnapshotClient(base, {
        fetch: serve(base).fetcher,
        cacheStorage: storage,
      })
      await original.load()
      await persisted(storage)
      const cache = await namespace(storage)
      const selection = (await cache.keys()).find((request) =>
        request.url.includes("/adopted-index-jp/"),
      )
      const older = selection && (await cache.match(selection))
      if (!selection || !older) throw new Error("missing adopted selection")
      await cache.delete(selection)
      const served = serve(base)
      const index = objectValue(parseJson(served.files.get(INDEX) ?? ""))
      served.files.set(
        INDEX,
        canonical({
          ...index,
          revision,
          current: { ...objectValue(index["current"]), published_at: "2026-09-30T00:00:00Z" },
        }),
      )
      const client = createSnapshotClient(base, { fetch: served.fetcher, cacheStorage: storage })
      await client.load()
      expect(client.status()).toMatchObject({ state: "error", kind: "corrupt" })
      // Another page writes the earlier verified selection before this client retries.
      await cache.put(selection, older)
      served.files.set(INDEX, canonical(index))
      await client.retry()
      await client.reload()
      expect(client.status()).toMatchObject({
        state: "ready",
        unconfirmed: true,
        updateError: { kind: "corrupt", detail: expect.stringContaining(detail) as string },
      })
      expect(client.snapshot()).toStrictEqual(original.snapshot())
    },
  )

  it.each([
    { change: { index_format: 3 }, kind: "incompatible" },
    { change: { current: null }, kind: "corrupt" },
    { change: { revision: 1 }, kind: "corrupt" },
    { change: { revision: 2, extra: true }, kind: "corrupt" },
  ])("reports $kind from background index validation", async ({ change, kind }) => {
    const storage = memoryCache()
    const original = createSnapshotClient(base, {
      fetch: serve(base).fetcher,
      cacheStorage: storage,
    })
    await original.load()
    await persisted(storage)
    const served = serve(base)
    const index = objectValue(parseJson(served.files.get(INDEX) ?? ""))
    served.files.set(INDEX, canonical({ ...index, ...change }))
    const client = createSnapshotClient(base, { fetch: served.fetcher, cacheStorage: storage })
    await client.load()
    await client.reload()
    expect(client.status()).toMatchObject({
      state: "ready",
      unconfirmed: true,
      outdated: true,
      updateError: { kind },
    })
    expect(client.snapshot()).toStrictEqual(original.snapshot())
    expect(served.requests).toEqual([INDEX])
  })

  it("reports incompatible when neither indexed version can replace local data", async () => {
    const storage = memoryCache()
    const original = createSnapshotClient(base, {
      fetch: serve(base).fetcher,
      cacheStorage: storage,
    })
    await original.load()
    await persisted(storage)
    const served = serve(base)
    const index = objectValue(parseJson(served.files.get(INDEX) ?? ""))
    served.files.set(
      INDEX,
      canonical({
        ...index,
        revision: 3,
        current: { ...objectValue(index["current"]), format_version: "99.0.0" },
        previous: null,
      }),
    )
    const client = createSnapshotClient(base, { fetch: served.fetcher, cacheStorage: storage })
    await client.load()
    await client.reload()
    expect(client.status()).toMatchObject({
      state: "ready",
      outdated: true,
      updateError: { kind: "incompatible" },
    })
    expect(client.snapshot()).toStrictEqual(original.snapshot())
  })

  it("keeps local data when the new manifest fails verification", async () => {
    const storage = memoryCache()
    const original = createSnapshotClient(base, {
      fetch: serve(base).fetcher,
      cacheStorage: storage,
    })
    await original.load()
    await persisted(storage)
    const served = serve(base)
    const index = objectValue(parseJson(served.files.get(INDEX) ?? ""))
    const hash = `sha256:${"0".repeat(64)}`
    const path = `snapshots/manifests/${hash.slice(7)}.json`
    served.files.set(path, canonical({}))
    served.files.set(
      INDEX,
      canonical({
        ...index,
        revision: 3,
        current: { ...objectValue(index["current"]), manifest_sha256: hash, manifest_path: path },
      }),
    )
    const client = createSnapshotClient(base, { fetch: served.fetcher, cacheStorage: storage })
    await client.load()
    const local = client.snapshot()
    await client.reload()
    expect(client.status()).toMatchObject({
      state: "ready",
      outdated: true,
      updateError: { kind: "corrupt" },
    })
    expect(client.snapshot()).toBe(local)
    expect(await records(storage)).toHaveLength(1)
    const oldRaw = await storage.open(`sve-raw:${encodeURIComponent(base)}`)
    expect((await oldRaw.keys()).length).toBeGreaterThan(0)
    const offline = serve(base)
    offline.fail(() => true)
    const restored = createSnapshotClient(base, { fetch: offline.fetcher, cacheStorage: storage })
    await restored.load()
    expect(restored.snapshot()?.manifestHash).toBe(local?.manifestHash)
  })

  it.each(["missing", "damaged"])(
    "falls back to the index when the selection is %s",
    async (condition) => {
      const storage = memoryCache()
      const original = createSnapshotClient(base, {
        fetch: serve(base).fetcher,
        cacheStorage: storage,
      })
      await original.load()
      await persisted(storage)
      const cache = await namespace(storage)
      const key = (await cache.keys()).find((request) => request.url.includes("/adopted-index-jp/"))
      if (!key) throw new Error("missing selection")
      if (condition === "missing") await cache.delete(key)
      else {
        const response = await cache.match(key)
        if (!response) throw new Error("missing selection response")
        await cache.put(key, new Response("damaged", { headers: response.headers }))
      }
      const served = serve(base)
      const client = createSnapshotClient(base, { fetch: served.fetcher, cacheStorage: storage })
      const seen = phases(client)
      await client.load()
      expect(seen).toEqual(["index", "ready"])
      expect(served.requests).toEqual([INDEX])
      expect(client.status()).toEqual(original.status())
    },
  )
})
