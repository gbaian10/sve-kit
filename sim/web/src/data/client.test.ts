// @vitest-environment node
import { describe, expect, it } from "vitest"

import { buildSnapshot } from "../../scripts/fixture/build"
import { memoryCache } from "../test-utils/cache"
import { type Fetcher } from "./cdn"
import { createSnapshotClient, type SnapshotStatus } from "./client"
import { canonical, type JsonObject, objectValue, parseJson, stringValue } from "./format-v3/json"
import { transferDigest } from "./integrity"
import { VerifiedCache } from "./verified-cache"

const stubImage = ({ width, height, seed }: { width: number; height: number; seed: number }) =>
  Promise.resolve(
    new TextEncoder().encode(`stub-webp:${String(width)}x${String(height)}:${String(seed)}`),
  )
const built = await buildSnapshot({ encodeImage: stubImage })

const keyFor = (table: string, partition: string, owner: string | null = null): string => {
  const file = (built.manifest["files"] as JsonObject[]).find((f) =>
    (f["row_counts"] as JsonObject[]).some(
      (r) =>
        r["table"] === table &&
        r["partition"] === partition &&
        (r["owner"] as JsonObject)["id"] === owner,
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
  it("cancels the rest of the startup batch when one file fails", async () => {
    const served = serve("/cdn")
    const startupKeys = (built.manifest["files"] as JsonObject[])
      .filter((file) => file["role"] === "bootstrap")
      .map((file) => stringValue(file["path"]))
    const [failing, ...held] = startupKeys
    const cancelled: string[] = []
    const fetcher: Fetcher = (url, init) => {
      const path = url.slice("/cdn/".length)
      if (path === failing) return Promise.reject(new TypeError("Failed to fetch"))
      if (!held.includes(path)) return served.fetcher(url, init)
      return new Promise((_, reject) => {
        init?.signal?.addEventListener("abort", () => {
          cancelled.push(path)
          reject(new DOMException("aborted", "AbortError"))
        })
      })
    }
    const client = createSnapshotClient("/cdn", { fetch: fetcher })
    await client.load()
    expect(client.status().state).toBe("error")
    await new Promise((resolve) => setTimeout(resolve, 0))
    // Every held request either started and was aborted or never left the queue.
    expect(cancelled.length).toBeGreaterThan(0)
    expect(cancelled.every((path) => held.includes(path))).toBe(true)
  })

  it("leaves an earlier error as soon as a new load starts", async () => {
    const served = serve("/cdn")
    served.fail(() => true)
    const client = createSnapshotClient("/cdn", { fetch: served.fetcher })
    await client.load()
    expect(client.status().state).toBe("error")
    served.fail(() => false)
    const loading = client.load()
    expect(client.status().state).toBe("loading")
    await loading
    expect(client.status().state).toBe("ready")
  })

  it("fetches startup files together instead of one round trip at a time", async () => {
    const served = serve("/cdn")
    let inFlight = 0
    let peak = 0
    const slow: Fetcher = async (url, init) => {
      inFlight += 1
      peak = Math.max(peak, inFlight)
      await new Promise((resolve) => setTimeout(resolve, 5))
      try {
        return await served.fetcher(url, init)
      } finally {
        inFlight -= 1
      }
    }
    const client = createSnapshotClient("/cdn", { fetch: slow })
    await client.load()
    expect(client.status().state).toBe("ready")
    expect(peak).toBeGreaterThan(1)
  })
  it("selects the current compatible snapshot and loads config plus bootstrap", async () => {
    const served = serve("/cdn")
    const client = createSnapshotClient("/cdn", { fetch: served.fetcher })
    const seen: SnapshotStatus[] = []
    client.subscribe(() => seen.push(client.status()))
    expect(client.status()).toEqual({ state: "idle" })
    await client.load()
    expect(client.status()).toEqual({ state: "ready", dataVersion: "20260929T000000Z-0001" })
    expect(seen.map((s) => (s.state === "loading" ? s.phase : s.state))).toEqual([
      "index",
      "manifest",
      "bootstrap",
      "ready",
    ])
    const snapshot = client.snapshot()
    expect(snapshot?.bootstrap.some((fragment) => fragment.table === "card")).toBe(true)
    expect(snapshot?.config["search"]).toEqual({
      grammar_version: "synthetic-v1",
      normalizer_version: "synthetic-v1",
    })
    expect(served.requests.some((path) => path.includes("0000000000000000"))).toBe(false)
    expect(
      served.requests.some((path) => path.includes("programs") || path.startsWith("images/")),
    ).toBe(false)
    await client.load()
    expect(served.requests.filter((path) => path === "snapshots/versions/index.json")).toHaveLength(
      1,
    )
  })

  it("reports incompatible when current and previous have no readable entry", async () => {
    const files = new Map(built.files)
    const index = parseJson(files.get("snapshots/versions/index.json") ?? "") as JsonObject
    files.set(
      "snapshots/versions/index.json",
      new TextEncoder().encode(
        JSON.stringify({
          ...index,
          current: { ...(index["current"] as JsonObject), format_version: "4.0.0" },
          previous: null,
        }),
      ),
    )
    const client = createSnapshotClient("/cdn", { fetch: serve("/cdn", files).fetcher })
    await client.load()
    expect(client.status()).toMatchObject({ state: "error", kind: "incompatible" })
    expect(client.snapshot()).toBeNull()
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

  it("retries a hash mismatch once, then reports corruption", async () => {
    const served = serve("/cdn")
    const manifestPath = built.manifestPath
    served.files.set(manifestPath, new TextEncoder().encode("{}"))
    const client = createSnapshotClient("/cdn", { fetch: served.fetcher })
    await client.load()
    expect(client.status()).toMatchObject({ state: "error", kind: "corrupt" })
    expect(served.requests.filter((path) => path === manifestPath)).toHaveLength(2)
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

  it("reports a network failure and recovers on retry", async () => {
    const served = serve("/cdn")
    served.fail((path) => path.startsWith("snapshots/blobs/"))
    const client = createSnapshotClient("/cdn", { fetch: served.fetcher })
    await client.load()
    expect(client.status()).toMatchObject({ state: "error", kind: "network" })
    served.fail(() => false)
    await client.retry()
    expect(client.status()).toMatchObject({ state: "ready" })
  })

  it("rejects a version index whose revision went backwards", async () => {
    const served = serve("/cdn")
    const client = createSnapshotClient("/cdn", { fetch: served.fetcher })
    await client.load()
    const index = parseJson(served.files.get("snapshots/versions/index.json") ?? "") as JsonObject
    served.files.set(
      "snapshots/versions/index.json",
      new TextEncoder().encode(JSON.stringify({ ...index, revision: 1 })),
    )
    await client.reload()
    // A reload failure keeps the active snapshot; the regression shows up as the update error.
    expect(client.status()).toMatchObject({
      state: "ready",
      updateError: { kind: "corrupt", detail: expect.stringContaining("backwards") as string },
    })
  })

  it("loads detail files on demand, joins them onto the bootstrap and keeps an LRU", async () => {
    const served = serve("/cdn")
    const client = createSnapshotClient("/cdn", { fetch: served.fetcher, detailCacheSize: 1 })
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
    await client.fragments(printingKey)
    expect(served.requests.length).toBe(before + 2)
    await expect(client.fragments("programs")).rejects.toThrow("not a detail file")
    await expect(client.fragments("nope")).rejects.toThrow("unknown file")
  })

  it("keeps two roots apart", async () => {
    const served = serve("/cdn")
    const other = serve("/cdn-preview")
    const client = createSnapshotClient("/cdn", { fetch: served.fetcher })
    const preview = createSnapshotClient("/cdn-preview", { fetch: other.fetcher })
    await client.load()
    expect(client.status().state).toBe("ready")
    expect(preview.status()).toEqual({ state: "idle" })
    expect(other.requests).toHaveLength(0)
  })

  it("reads a preview root through its pointer file", async () => {
    const files = new Map(built.files)
    const manifestHash = `sha256:${built.manifestPath.slice("snapshots/manifests/".length, -".json".length)}`
    files.set(
      "snapshots/preview/current.json",
      new TextEncoder().encode(
        JSON.stringify({
          manifest_path: built.manifestPath,
          manifest_sha256: manifestHash,
          data_version: "preview-20260929T000000Z-0001",
        }),
      ),
    )
    const client = createSnapshotClient("/cdn-preview", {
      fetch: serve("/cdn-preview", files).fetcher,
      entry: "preview",
    })
    await client.load()
    expect(client.status()).toMatchObject({ state: "ready" })
  })
})

describe("createSnapshotClient: reload and transport edge cases", () => {
  it("keeps the active snapshot when a reload fails and reports the failure", async () => {
    const served = serve("/cdn")
    const client = createSnapshotClient("/cdn", { fetch: served.fetcher })
    await client.load()
    const before = client.snapshot()
    const index = parseJson(served.files.get("snapshots/versions/index.json") ?? "") as JsonObject
    served.files.set(
      "snapshots/versions/index.json",
      new TextEncoder().encode(JSON.stringify({ ...index, index_format: 3 })),
    )
    await client.reload()
    expect(client.status()).toMatchObject({
      state: "ready",
      dataVersion: before?.dataVersion,
      updateError: { kind: "incompatible" },
    })
    expect(client.snapshot()).toBe(before)
    expect((await client.fragments(printingKey)).length).toBeGreaterThan(0)
    served.fail((path) => path === "snapshots/versions/index.json")
    await client.reload()
    expect(client.status()).toMatchObject({ state: "ready", updateError: { kind: "network" } })
    served.fail(() => false)
    served.files.set(
      "snapshots/versions/index.json",
      new TextEncoder().encode(JSON.stringify(index)),
    )
    await client.reload()
    expect(client.status()).toEqual({ state: "ready", dataVersion: before?.dataVersion })
  })

  it("only checks the index when a reload finds the same manifest", async () => {
    const served = serve("/cdn")
    const client = createSnapshotClient("/cdn", { fetch: served.fetcher })
    await client.load()
    const before = client.snapshot()
    const loaded = served.requests.length
    const seen: SnapshotStatus[] = []
    client.subscribe(() => seen.push(client.status()))
    await client.reload()
    expect(served.requests.slice(loaded)).toEqual(["snapshots/versions/index.json"])
    expect(client.snapshot()).toBe(before)
    expect(seen.map((status) => (status.state === "ready" ? status.updating : undefined))).toEqual([
      "index",
      undefined,
    ])
    expect(client.status()).toEqual({ state: "ready", dataVersion: before?.dataVersion })
  })

  it("downloads again when the pointer names a different manifest", async () => {
    const files = new Map(built.files)
    const manifestHash = `sha256:${built.manifestPath.slice("snapshots/manifests/".length, -".json".length)}`
    const pointer = (hash: string) =>
      new TextEncoder().encode(
        JSON.stringify({
          manifest_path: built.manifestPath,
          manifest_sha256: hash,
          data_version: "preview-20260929T000000Z-0001",
        }),
      )
    files.set("snapshots/preview/current.json", pointer(manifestHash))
    const served = serve("/cdn-preview", files)
    const client = createSnapshotClient("/cdn-preview", { fetch: served.fetcher, entry: "preview" })
    await client.load()
    const before = client.snapshot()
    files.set("snapshots/preview/current.json", pointer(`sha256:${"0".repeat(64)}`))
    const loaded = served.requests.length
    await client.reload()
    // A different hash is fetched and checked instead of being skipped as unchanged.
    expect(served.requests.slice(loaded)).toContain(built.manifestPath)
    expect(client.snapshot()).toBe(before)
    expect(client.status()).toHaveProperty("updateError")
  })

  it("treats a broken index_format as corrupt, not as an update prompt", async () => {
    const files = new Map(built.files)
    const index = parseJson(files.get("snapshots/versions/index.json") ?? "") as JsonObject
    const { index_format: _format, ...withoutFormat } = index
    files.set(
      "snapshots/versions/index.json",
      new TextEncoder().encode(JSON.stringify(withoutFormat)),
    )
    const client = createSnapshotClient("/cdn", { fetch: serve("/cdn", files).fetcher })
    await client.load()
    expect(client.status()).toMatchObject({ state: "error", kind: "corrupt" })
  })

  it("treats an unsupported index format as incompatible, not corrupt", async () => {
    const files = new Map(built.files)
    const index = parseJson(files.get("snapshots/versions/index.json") ?? "") as JsonObject
    files.set(
      "snapshots/versions/index.json",
      new TextEncoder().encode(JSON.stringify({ ...index, index_format: 3 })),
    )
    const client = createSnapshotClient("/cdn", { fetch: serve("/cdn", files).fetcher })
    await client.load()
    expect(client.status()).toMatchObject({ state: "error", kind: "incompatible" })
  })

  it("classifies a body that fails to arrive as a network error", async () => {
    const fetcher: Fetcher = () =>
      Promise.resolve({
        ok: true,
        status: 200,
        arrayBuffer: () => Promise.reject(new TypeError("body stream aborted")),
      } as unknown as Response)
    const client = createSnapshotClient("/cdn", { fetch: fetcher })
    await client.load()
    expect(client.status()).toMatchObject({
      state: "error",
      kind: "network",
      detail: "body stream aborted",
    })
  })

  it("shares an in-flight detail request even when the LRU is full", async () => {
    const served = serve("/cdn")
    const client = createSnapshotClient("/cdn", { fetch: served.fetcher, detailCacheSize: 1 })
    await client.load()
    const first = client.fragments(printingKey)
    const other = client.fragments(textKey)
    const again = client.fragments(printingKey)
    expect(again).toBe(first)
    await Promise.all([first, other, again])
    const bp01Path = stringValue(client.snapshot()?.files.get(printingKey)?.["path"])
    expect(served.requests.filter((path) => path === bp01Path)).toHaveLength(1)
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
          return (await cache.keys()).some((request) => request.url.includes("/adopted-index/"))
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

  it("restores the same manifest without fetching or decoding the manifest and startup files", async () => {
    const storage = memoryCache()
    const original = createSnapshotClient(base, {
      fetch: serve(base).fetcher,
      cacheStorage: storage,
    })
    await original.load()
    await persisted(storage)
    const served = serve(base)
    const client = createSnapshotClient(base, { fetch: served.fetcher, cacheStorage: storage })
    const seen = phases(client)
    await client.load()
    expect(served.requests).toEqual([INDEX])
    expect(seen[0]).toBe("ready")
    await client.reload()
    expect(client.status()).toEqual(original.status())
    expect(client.snapshot()).toStrictEqual(original.snapshot())
    expect(await client.fragments(textKey)).toStrictEqual(await original.fragments(textKey))
  })

  it("publishes local data and resolves load before a held index can answer", async () => {
    const storage = memoryCache()
    const original = createSnapshotClient(base, {
      fetch: serve(base).fetcher,
      cacheStorage: storage,
    })
    await original.load()
    await persisted(storage)
    const served = serve(base)
    let release: () => void = () => undefined
    const held = new Promise<void>((resolve) => {
      release = resolve
    })
    const client = createSnapshotClient(base, {
      cacheStorage: storage,
      fetch: async (url, init) => {
        if (url.endsWith(INDEX)) await held
        return served.fetcher(url, init)
      },
    })
    const seen: SnapshotStatus[] = []
    client.subscribe(() => seen.push(client.status()))
    await client.load()
    expect(seen[0]).toMatchObject({ state: "ready", unconfirmed: true })
    expect(client.snapshot()).toStrictEqual(original.snapshot())
    expect(client.status()).toMatchObject({ state: "ready", updating: "index", unconfirmed: true })
    const local = client.snapshot()
    release()
    await client.reload()
    expect(served.requests).toEqual([INDEX])
    expect(client.snapshot()).toBe(local)
    expect(client.status()).toEqual({ state: "ready", dataVersion: local?.dataVersion })
  })

  it("rechecks after reconnection during restore and makes reload await confirmation", async () => {
    const storage = memoryCache()
    const original = createSnapshotClient(base, {
      fetch: serve(base).fetcher,
      cacheStorage: storage,
    })
    await original.load()
    await persisted(storage)
    let resumeRestore: () => void = () => undefined
    let notifyRestore: () => void = () => undefined
    let resumeIndex: () => void = () => undefined
    const restoring = new Promise<void>((resolve) => {
      notifyRestore = resolve
    })
    const heldRestore = new Promise<void>((resolve) => {
      resumeRestore = resolve
    })
    const heldIndex = new Promise<void>((resolve) => {
      resumeIndex = resolve
    })
    const open = storage.open.bind(storage)
    storage.open = async (name) => {
      const cache = await open(name)
      const match = cache.match.bind(cache)
      cache.match = async (key, options) => {
        if ((key instanceof Request ? key.url : String(key)).includes("/adopted-index/")) {
          notifyRestore()
          await heldRestore
        }
        return match(key, options)
      }
      return cache
    }
    const served = serve(base)
    let online = false
    const client = createSnapshotClient(base, {
      cacheStorage: storage,
      fetch: async (url, init) => {
        if (url.endsWith(INDEX)) {
          if (!online) throw new TypeError("offline during startup")
          await heldIndex
        }
        return served.fetcher(url, init)
      },
    })
    const loading = client.load()
    await restoring
    // The application's online handler calls reload while the local record is still being read.
    online = true
    let reloaded = false
    const reloading = client.reload().then(() => {
      reloaded = true
    })
    const coalesced = client.reload()
    resumeRestore()
    await loading
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(reloaded).toBe(false)
    expect(client.status()).toMatchObject({ state: "ready", unconfirmed: true, updating: "index" })
    expect(client.snapshot()).toStrictEqual(original.snapshot())
    resumeIndex()
    await Promise.all([reloading, coalesced])
    expect(client.status()).toEqual(original.status())
    expect(served.requests).toEqual([INDEX])
  })

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
      const stored = new VerifiedCache(base, "snapshot", 1, storage)
      if (failure === "adoption") {
        const record = objectValue(parseJson(originalBytes))
        const manifest = objectValue(record["manifest"])
        const files = manifest["files"] as JsonObject[]
        // This is readable by catalog restore but fails when constructing image metadata at adoption.
        await stored.write(
          "snapshot",
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
      await stored.write("snapshot", snapshot.manifestHash, originalBytes)
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
        request.url.includes("/adopted-index/"),
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

  it("keeps local data usable offline and clears its warning after reconnection", async () => {
    const storage = memoryCache()
    const original = createSnapshotClient(base, {
      fetch: serve(base).fetcher,
      cacheStorage: storage,
    })
    await original.load()
    await persisted(storage)
    const served = serve(base)
    served.fail((path) => path === INDEX)
    const client = createSnapshotClient(base, { fetch: served.fetcher, cacheStorage: storage })
    await client.load()
    await client.reload()
    expect(client.snapshot()).toStrictEqual(original.snapshot())
    expect(client.status()).toMatchObject({
      state: "ready",
      unconfirmed: true,
      outdated: true,
      updateError: { kind: "network" },
    })
    served.fail(() => false)
    await client.reload()
    expect(client.status()).toEqual(original.status())
  })

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
        current: { ...objectValue(index["current"]), format_version: "4.0.0" },
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
  })

  it("keeps the local snapshot until a newer background download is fully verified", async () => {
    const storage = memoryCache()
    const original = createSnapshotClient(base, {
      fetch: serve(base).fetcher,
      cacheStorage: storage,
    })
    await original.load()
    await persisted(storage)
    const served = serve(base)
    const nextManifest = {
      ...built.manifest,
      data_version: "20260930T000000Z-0001",
      published_at: "2026-09-30T00:00:00Z",
    }
    const bytes = canonical(nextManifest)
    const hash = await transferDigest(bytes)
    const path = `snapshots/manifests/${hash.slice(7)}.json`
    served.files.set(path, bytes)
    const index = objectValue(parseJson(served.files.get(INDEX) ?? ""))
    const current = objectValue(index["current"])
    served.files.set(
      INDEX,
      canonical({
        ...index,
        revision: 3,
        previous: current,
        current: {
          ...current,
          data_version: nextManifest.data_version,
          published_at: nextManifest.published_at,
          manifest_path: path,
          manifest_sha256: hash,
        },
      }),
    )
    let release: () => void = () => undefined
    const held = new Promise<void>((resolve) => {
      release = resolve
    })
    const client = createSnapshotClient(base, {
      cacheStorage: storage,
      fetch: async (url, init) => {
        if (url.endsWith(path)) await held
        return served.fetcher(url, init)
      },
    })
    await client.load()
    await expect.poll(() => client.status()).toMatchObject({ state: "ready", updating: "manifest" })
    expect(client.snapshot()).toStrictEqual(original.snapshot())
    release()
    await client.reload()
    expect(client.status()).toEqual({ state: "ready", dataVersion: nextManifest.data_version })
    expect(client.snapshot()?.manifestHash).toBe(hash)
    // The adopted selection on the next page is the completed new snapshot.
    await expect
      .poll(async () => {
        const cache = await namespace(storage)
        const key = (await cache.keys()).find((request) => request.url.includes("/adopted-index/"))
        const response = key && (await cache.match(key))
        if (!response) return undefined
        return objectValue(parseJson(await response.text()))["manifestHash"]
      })
      .toBe(hash)
    const next = createSnapshotClient(base, { fetch: served.fetcher, cacheStorage: storage })
    await next.load()
    expect(next.snapshot()?.manifestHash).toBe(hash)
    await next.reload()
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
      const key = (await cache.keys()).find((request) => request.url.includes("/adopted-index/"))
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

  it("records the selection of a restored snapshot without storing the snapshot again", async () => {
    const storage = memoryCache()
    const original = createSnapshotClient(base, {
      fetch: serve(base).fetcher,
      cacheStorage: storage,
    })
    await original.load()
    await persisted(storage)
    const cache = await namespace(storage)
    const selection = (await cache.keys()).find((request) => request.url.includes("/adopted-"))
    if (!selection) throw new Error("missing selection")
    await cache.delete(selection)
    const writes: string[] = []
    const open = storage.open.bind(storage)
    storage.open = async (name) => {
      const opened = await open(name)
      const put = opened.put.bind(opened)
      opened.put = (key, response) => {
        writes.push(key instanceof Request ? key.url : String(key))
        return put(key, response)
      }
      return opened
    }
    const served = serve(base)
    const client = createSnapshotClient(base, { fetch: served.fetcher, cacheStorage: storage })
    await client.load()
    await persisted(storage)
    expect(served.requests).toEqual([INDEX])
    expect(writes).toEqual([expect.stringContaining("/adopted-index/")])
  })

  it("keeps a newer index check when an earlier snapshot write finishes later", async () => {
    const storage = memoryCache()
    let release: () => void = () => undefined
    const held = new Promise<void>((resolve) => {
      release = resolve
    })
    const open = storage.open.bind(storage)
    storage.open = async (name) => {
      const opened = await open(name)
      const put = opened.put.bind(opened)
      opened.put = async (key, response) => {
        if (!(key instanceof Request ? key.url : String(key)).includes("/adopted-")) await held
        return put(key, response)
      }
      return opened
    }
    const revision = async () => {
      const cache = await namespace(storage)
      const key = (await cache.keys()).find((request) => request.url.includes("/adopted-index/"))
      const response = key && (await cache.match(key))
      return response ? objectValue(parseJson(await response.text()))["revision"] : undefined
    }
    const served = serve(base)
    const client = createSnapshotClient(base, { fetch: served.fetcher, cacheStorage: storage })
    await client.load()
    const index = objectValue(parseJson(served.files.get(INDEX) ?? ""))
    served.files.set(INDEX, canonical({ ...index, revision: 3 }))
    await client.reload()
    await expect.poll(revision).toBe(3)
    release()
    await expect.poll(async () => (await records(storage)).length).toBe(1)
    await new Promise((resolve) => setTimeout(resolve, 50))
    expect(await revision()).toBe(3)
  })

  it("a damaged record falls back to the full verified download", async () => {
    const storage = memoryCache()
    const original = createSnapshotClient(base, {
      fetch: serve(base).fetcher,
      cacheStorage: storage,
    })
    await original.load()
    await persisted(storage)
    const cache = await namespace(storage)
    const [record] = await records(storage)
    const response = record && (await cache.match(record))
    if (!record || !response) throw new Error("missing record")
    const bytes = new Uint8Array(await response.arrayBuffer())
    const middle = bytes.length >> 1
    bytes[middle] = (bytes[middle] ?? 0) ^ 0x01
    await cache.put(record, new Response(bytes, { headers: response.headers }))
    const served = serve(base)
    const client = createSnapshotClient(base, { fetch: served.fetcher, cacheStorage: storage })
    await client.load()
    expect(served.requests.length).toBeGreaterThan(2)
    expect(client.snapshot()).toStrictEqual(original.snapshot())
  })

  it("still rejects an index entry that differs from the stored manifest", async () => {
    const storage = memoryCache()
    const original = createSnapshotClient(base, {
      fetch: serve(base).fetcher,
      cacheStorage: storage,
    })
    await original.load()
    await persisted(storage)
    const served = serve(base)
    const index = parseJson(served.files.get(INDEX) ?? new Uint8Array()) as JsonObject
    const current = { ...(index["current"] as JsonObject), published_at: "2026-09-30T00:00:00Z" }
    served.files.set(
      INDEX,
      new TextEncoder().encode(JSON.stringify({ ...index, revision: 3, current })),
    )
    const client = createSnapshotClient(base, { fetch: served.fetcher, cacheStorage: storage })
    await client.load()
    await client.reload()
    expect(client.status()).toMatchObject({
      state: "ready",
      unconfirmed: true,
      updateError: { kind: "corrupt" },
    })
    expect(served.requests).toEqual([INDEX])
  })

  it("stores nothing for a download that fails verification", async () => {
    const storage = memoryCache()
    const files = new Map(built.files)
    const bootstrap = (built.manifest["files"] as JsonObject[]).find(
      (file) => file["role"] === "bootstrap",
    )
    files.set(stringValue(bootstrap?.["path"]), new Uint8Array([1, 2, 3]))
    const client = createSnapshotClient(base, {
      fetch: serve(base, files).fetcher,
      cacheStorage: storage,
    })
    await client.load()
    expect(client.status()).toMatchObject({ state: "error" })
    await new Promise((resolve) => setTimeout(resolve, 50))
    expect(await records(storage)).toEqual([])
  })

  it("rejects changed entry fields even when reusing the snapshot already in memory", async () => {
    const served = serve(base)
    const client = createSnapshotClient(base, {
      fetch: served.fetcher,
      cacheStorage: memoryCache(),
    })
    await client.load()
    const snapshot = client.snapshot()
    const index = parseJson(served.files.get(INDEX) ?? new Uint8Array()) as JsonObject
    const current = { ...(index["current"] as JsonObject), published_at: "2026-09-30T00:00:00Z" }
    served.files.set(
      INDEX,
      new TextEncoder().encode(JSON.stringify({ ...index, revision: 3, current })),
    )
    await client.reload()
    expect(client.status()).toMatchObject({ state: "ready", updateError: { kind: "corrupt" } })
    expect(client.snapshot()).toBe(snapshot)
  })
})
