// @vitest-environment node
import { describe, expect, it, vi } from "vitest"

import { memoryCache as storage } from "../test-utils/cache"
import type { Fetcher } from "./cdn"
import { createSnapshotClient } from "./client"
import { canonical, type JsonObject, type JsonValue, stringValue } from "./format-v1/json"
import { digest } from "./format-v1/sha256"
import { loadImagePage } from "./images"
import { MetadataBytes } from "./metadata"
import { requestQueue } from "./request-queue"

const golden = import.meta.glob<string>(
  "../../../../tests/fixtures/snapshot-contract/v2/**/*.json",
  { query: "?raw", import: "default", eager: true },
)
const value = (name: string): JsonValue => {
  const key = Object.keys(golden).find((path) => path.endsWith(`/v2/${name}`))
  if (!key) throw new Error("missing golden")
  return JSON.parse(golden[key] ?? "") as JsonValue
}
const manifest = value("manifest.json") as JsonObject
const rawManifest = canonical(manifest)
const source = new Map(
  (manifest["files"] as JsonObject[]).map((file) => [
    stringValue(file["path"]),
    canonical(value(`payloads/${stringValue(file["sha256"]).slice(7)}.json`)),
  ]),
)
const manifestPath = `snapshots/manifests/${digest(rawManifest).slice(7)}.json`
source.set(manifestPath, rawManifest)
source.set(
  "snapshots/preview/current.json",
  canonical({ manifest_path: manifestPath, manifest_sha256: digest(rawManifest) }),
)
const face = { printingId: "p:a", faceId: "f:a" }

function serve(bytes = source) {
  const paths: string[] = []
  const fetcher: Fetcher = (url) => {
    const path = url.slice("/cdn/".length)
    paths.push(path)
    const data = bytes.get(path)
    return Promise.resolve(
      data ? new Response(data.slice().buffer) : new Response(null, { status: 404 }),
    )
  }
  return { fetcher, paths }
}
const metadataFiles = (number: number) =>
  new Map(
    Array.from({ length: number }, (_, i) => {
      const bytes = canonical({ synthetic: i })
      return [
        `file${String(i)}`,
        {
          key: `file${String(i)}`,
          path: `data${String(i)}`,
          sha256: digest(bytes),
          bytes: bytes.length,
        },
      ] as const
    }),
  )

describe("page image metadata", () => {
  it("locates the printing owner media; does not download unrelated files or picture blobs", async () => {
    const served = serve()
    const client = createSnapshotClient("/cdn", {
      entry: "preview",
      fetch: served.fetcher,
      cacheStorage: storage(),
    })
    await client.load()
    expect(client.snapshot()).not.toBeNull()
    const before = served.paths.length
    const page = await loadImagePage(client, [face, face])
    expect(page.cardImage("p:a", "f:a")?.src).toBe("/cdn/images/card_l/1.webp?v=7")
    expect(page.asset("p:a", "f:a")?.["image_id"]).toBe("img:a")
    expect(page.asset("p:b", "f:b")).toBeUndefined()
    expect(served.paths.slice(before)).toHaveLength(1)
    expect(new Set(served.paths.slice(before)).size).toBe(1)
    expect(served.paths.some((path) => path.startsWith("images/"))).toBe(false)
    await client.prefetchImages()
    const done = served.paths.length
    await loadImagePage(client, [{ printingId: "p:b", faceId: "f:b" }])
    await loadImagePage(client, [face])
    expect(served.paths).toHaveLength(done)
    expect(client.metadataStatus()).toMatchObject({ state: "complete", persistent: true })
  })
  it("reuses decoded faces on overlapping pages and re-derives only evicted faces", async () => {
    const served = serve()
    const client = createSnapshotClient("/cdn", {
      entry: "preview",
      fetch: served.fetcher,
      cacheStorage: storage(),
    })
    await client.load()
    const parse = vi.spyOn(client, "fragments")
    const first = await loadImagePage(client, [face])
    expect(first.cardImage("p:a", "f:a")).toBeDefined()
    const calls = parse.mock.calls.length
    await loadImagePage(client, [face])
    expect(parse).toHaveBeenCalledTimes(calls)
    await loadImagePage(client, [face, { printingId: "p:b", faceId: "f:b" }])
    const overlapping = parse.mock.calls.length
    await loadImagePage(client, [face])
    expect(parse).toHaveBeenCalledTimes(overlapping)
    // Known negative entries consume the same finite face budget as successful ones.
    for (let i = 0; i < 65; i += 1)
      await loadImagePage(client, [
        { printingId: `unbound${String(i)}`, faceId: `face${String(i)}` },
      ])
    await loadImagePage(client, [face])
    expect(parse.mock.calls.length).toBeGreaterThan(overlapping)
  })
  it("does not background-download anything without persistent storage", async () => {
    let requests = 0
    const bytes = new MetadataBytes(
      "/cdn",
      "no-storage",
      metadataFiles(70),
      () => {
        requests += 1
        return Promise.resolve(new Response(canonical({ synthetic: 0 }).slice().buffer))
      },
      () => undefined,
    )
    await bytes.prefetch()
    expect(requests).toBe(0)
    expect(bytes.status()).toMatchObject({ persistent: false, done: 0, state: "idle" })
    await bytes.read("file0")
    expect(requests).toBe(1)
  })
  it("prunes only old metadata namespaces and keeps the active and immediately previous version", async () => {
    const cache = storage()
    await cache.open("unrelated-app")
    await cache.open("sve-images-%2Fcdn-preview:-preview")
    const fetcher: Fetcher = () =>
      Promise.resolve(new Response(canonical({ synthetic: 0 }).slice().buffer))
    const first = new MetadataBytes("/cdn", "a", metadataFiles(1), fetcher, () => undefined, cache)
    await first.prefetch()
    const second = new MetadataBytes(
      "/cdn",
      "b",
      metadataFiles(1),
      fetcher,
      () => undefined,
      cache,
      fetcher,
      "a",
    )
    await second.prefetch()
    expect((await cache.keys()).sort()).toEqual(
      [
        "sve-images-%2Fcdn:-a",
        "sve-images-%2Fcdn:-b",
        "sve-images-%2Fcdn-preview:-preview",
        "unrelated-app",
      ].sort(),
    )
    const third = new MetadataBytes(
      "/cdn",
      "c",
      metadataFiles(1),
      fetcher,
      () => undefined,
      cache,
      fetcher,
      "b",
    )
    await third.prefetch()
    expect((await cache.keys()).sort()).toEqual(
      [
        "sve-images-%2Fcdn:-b",
        "sve-images-%2Fcdn:-c",
        "sve-images-%2Fcdn-preview:-preview",
        "unrelated-app",
      ].sort(),
    )
  })
  it("revalidates equal-length persistent bytes instead of accepting a corrupt cache entry", async () => {
    const cache = storage()
    const opened = await cache.open("sve-images-%2Fcdn:-corrupt")
    await opened.put("/cdn/data0", new Response(canonical({ synthetic: 9 }).slice().buffer))
    let requests = 0
    const bytes = new MetadataBytes(
      "/cdn",
      "corrupt",
      metadataFiles(1),
      () => {
        requests += 1
        return Promise.resolve(new Response(canonical({ synthetic: 0 }).slice().buffer))
      },
      () => undefined,
      cache,
    )
    expect(await bytes.read("file0")).toEqual(canonical({ synthetic: 0 }))
    expect(requests).toBe(1)
  })
  it("successful snapshot adoption aborts old metadata requests, not merely their returned indexes", async () => {
    const served = serve()
    const imagePaths = new Set(
      (manifest["files"] as JsonObject[])
        .filter((f) => f["role"] === "images")
        .map((f) => `/cdn/${stringValue(f["path"])}`),
    )
    let aborted = 0
    let block = false
    const client = createSnapshotClient("/cdn", {
      entry: "preview",
      cacheStorage: storage(),
      fetch: (url, init) => {
        if (!block || !imagePaths.has(url)) return served.fetcher(url, init)
        return new Promise<Response>((_resolve, reject) => {
          init?.signal?.addEventListener(
            "abort",
            () => {
              aborted += 1
              reject(new Error("aborted"))
            },
            { once: true },
          )
        })
      },
    })
    await client.load()
    block = true
    const prefetch = client.prefetchImages()
    await new Promise((resolve) => setTimeout(resolve, 0))
    await client.reload()
    await prefetch
    expect(aborted).toBeGreaterThan(0)
  })
  it("reopens bytes from persistent cache after the 64-file memory LRU has been evicted", async () => {
    const files = metadataFiles(70)
    const requests: string[] = []
    const fetcher: Fetcher = (url) => {
      requests.push(url)
      const i = Number(url.split("data")[1])
      return Promise.resolve(new Response(canonical({ synthetic: i }).slice().buffer))
    }
    const sharedCache = storage()
    const bytes = new MetadataBytes(
      "/cdn",
      "snapshot-a",
      files,
      fetcher,
      () => undefined,
      sharedCache,
    )
    await bytes.prefetch()
    expect(requests).toHaveLength(70)
    await bytes.read("file0")
    expect(requests).toHaveLength(70)
    const reopened = new MetadataBytes(
      "/cdn",
      "snapshot-a",
      files,
      fetcher,
      () => undefined,
      sharedCache,
    )
    await reopened.read("file0")
    expect(requests).toHaveLength(70)
    const separate = new MetadataBytes(
      "/cdn",
      "snapshot-b",
      files,
      fetcher,
      () => undefined,
      sharedCache,
    )
    await separate.read("file0")
    expect(requests).toHaveLength(71)
  })
  it("reports quota degradation and retries after eviction rather than claiming persistent offline readiness", async () => {
    const files = metadataFiles(70)
    let requests = 0
    const served: Fetcher = (url) => {
      requests += 1
      return Promise.resolve(
        new Response(canonical({ synthetic: Number(url.split("data")[1]) }).slice().buffer),
      )
    }
    const bytes = new MetadataBytes("/cdn", "quota", files, served, () => undefined, storage(true))
    await bytes.prefetch()
    expect(bytes.status()).toMatchObject({ persistent: false, state: "cancelled" })
    expect(requests).toBeLessThanOrEqual(3)
    expect(await bytes.read("file0")).toEqual(canonical({ synthetic: 0 }))
  })
  it("rejects a replaced snapshot even when its transport ignores abort", async () => {
    let finish: ((response: Response) => void) | undefined
    const bytes = new MetadataBytes(
      "/cdn",
      "replaced",
      metadataFiles(1),
      () =>
        new Promise<Response>((resolve) => {
          finish = resolve
        }),
      () => undefined,
      storage(),
    )
    const reading = bytes.read("file0")
    const rejected = expect(reading).rejects.toThrow("snapshot replaced")
    await new Promise((resolve) => setTimeout(resolve, 0))
    bytes.dispose()
    if (!finish) throw new Error("transport did not start")
    finish(new Response(canonical({ synthetic: 0 }).slice().buffer))
    await rejected
    expect(bytes.status().done).toBe(0)
    await expect(bytes.read("file0")).rejects.toThrow("snapshot replaced")
  })
  it("a cancelled page cannot return an index or contaminate another snapshot", async () => {
    const served = serve()
    const client = createSnapshotClient("/cdn", { entry: "preview", fetch: served.fetcher })
    await client.load()
    const abort = new AbortController()
    abort.abort()
    const before = served.paths.length
    await expect(loadImagePage(client, [face], abort.signal)).rejects.toThrow(
      "image page cancelled",
    )
    expect(served.paths).toHaveLength(before)
  })
  it("rejects wrong hashes/lengths with a bounded retry and never caches the bad bytes", async () => {
    let requests = 0
    const bytes = new MetadataBytes(
      "/cdn",
      "bad",
      metadataFiles(1),
      () => {
        requests += 1
        return Promise.resolve(new Response("bad"))
      },
      () => undefined,
      storage(),
    )
    await expect(bytes.read("file0")).rejects.toThrow("blob-integrity")
    expect(requests).toBe(2)
    expect(bytes.status().done).toBe(0)
  })
})

describe("network scheduling", () => {
  it("reserves a fourth slot for the page while background bodies are stalled", async () => {
    const starts: string[] = []
    const release: (() => void)[] = []
    const queue = requestQueue(async (url) => {
      starts.push(url)
      await new Promise<void>((resolve) => {
        release.push(resolve)
      })
      return new Response("{}")
    })
    const background = Array.from({ length: 6 }, (_, i) =>
      queue.background(`background${String(i)}`),
    )
    await Promise.resolve()
    expect(starts).toHaveLength(3)
    const visible = queue.foreground("visible")
    expect(starts).toEqual(["background0", "background1", "background2", "visible"])
    while (starts.length < 7 || release.length > 0) {
      release.splice(0).forEach((resolve) => {
        resolve()
      })
      await new Promise((resolve) => setTimeout(resolve, 0))
    }
    await Promise.all([...background, visible])
  })
  it("promotes visible metadata ahead of queued background jobs in its own queue", async () => {
    const starts: string[] = []
    const release: Array<() => void> = []
    const bytes = new MetadataBytes(
      "/cdn",
      "priority",
      metadataFiles(10),
      async (url) => {
        starts.push(url)
        await new Promise<void>((resolve) => {
          release.push(resolve)
        })
        return new Response(canonical({ synthetic: Number(url.split("data")[1]) }).slice().buffer)
      },
      () => undefined,
      storage(),
    )
    const background = bytes.prefetch()
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(starts).toHaveLength(3)
    const visible = bytes.read("file9")
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(starts).toEqual(["/cdn/data0", "/cdn/data1", "/cdn/data2", "/cdn/data9"])
    bytes.cancel()
    release.splice(0).forEach((resolve) => {
      resolve()
    })
    await Promise.all([background, visible])
  })
  it("cancels pending background work, preserves verified files, and permits a bounded retry", async () => {
    const files = metadataFiles(7)
    let release: () => void = () => undefined
    const blocked = new Promise<void>((resolve) => {
      release = resolve
    })
    let calls = 0
    const bytes = new MetadataBytes(
      "/cdn",
      "cancel",
      files,
      async (url) => {
        calls += 1
        await blocked
        return new Response(canonical({ synthetic: Number(url.split("data")[1]) }).slice().buffer)
      },
      () => undefined,
      storage(),
    )
    const first = bytes.prefetch()
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(calls).toBe(3)
    bytes.cancel()
    release()
    await first
    expect(bytes.status().state).toBe("cancelled")
    await bytes.prefetch()
    expect(bytes.status()).toMatchObject({ state: "complete", done: 7 })
    expect(calls).toBe(7)
  })
})
