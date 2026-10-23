// @vitest-environment node
import { describe, expect, it, vi } from "vitest"

import { memoryCache as storage } from "../test-utils/cache"
import { imageSnapshot } from "../test-utils/image-snapshot"
import type { Fetcher } from "./cdn"
import { createSnapshotClient } from "./client"
import { canonical, type JsonObject, type JsonValue, stringValue } from "./format-v3/json"
import { digest } from "./format-v3/sha256"
import { loadImagePage, retainedImageIndex } from "./images"
import { MetadataBytes } from "./metadata"
import { requestQueue } from "./request-queue"

const golden = import.meta.glob<string>(
  "../../../../tests/fixtures/snapshot-contract/v3/**/*.json",
  { query: "?raw", import: "default", eager: true },
)
const value = (name: string): JsonValue => {
  const key = Object.keys(golden).find((path) => path.endsWith(`/v3/${name}`))
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
  it("reuses decoded faces on overlapping pages and ignores undeclared identities", async () => {
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
    // Arbitrary identities must not displace declared faces or enter the retained index.
    for (let i = 0; i < 65; i += 1)
      await loadImagePage(client, [
        { printingId: `unbound${String(i)}`, faceId: `face${String(i)}` },
      ])
    await loadImagePage(client, [face])
    expect(parse).toHaveBeenCalledTimes(overlapping)
    const snapshot = client.snapshot()
    if (!snapshot) throw new Error("snapshot missing")
    expect(retainedImageIndex(snapshot).known?.("unbound0", "face0")).toBe(false)
  })
  it("keeps more than 64 declared faces without rereading and rejects cross-card pairs", async () => {
    const { client, snapshot, faces } = imageSnapshot(80)
    const read = vi.spyOn(client, "fragments")
    for (let start = 0; start < faces.length; start += 12)
      await loadImagePage(client, faces.slice(start, start + 12))
    const reads = read.mock.calls.length
    const retained = retainedImageIndex(snapshot)
    expect(retained.cardImage("p:0", "f:0")?.src).toContain("/1.webp")
    await loadImagePage(client, faces.slice(0, 2))
    expect(read).toHaveBeenCalledTimes(reads)
    await loadImagePage(client, [{ printingId: "p:0", faceId: "f:1" }])
    expect(retained.known?.("p:0", "f:1")).toBe(false)
    expect(read).toHaveBeenCalledTimes(reads)
    expect(retainedImageIndex({ ...snapshot }).known?.("p:0", "f:0")).toBe(false)
  })
  it("retains a declared face with absent media as a known negative result", async () => {
    const { client, snapshot, faces } = imageSnapshot(1)
    const read = vi.spyOn(client, "fragments").mockResolvedValue([])
    await loadImagePage(client, faces)
    const retained = retainedImageIndex(snapshot)
    expect(retained.known?.("p:0", "f:0")).toBe(true)
    expect(retained.cardImage("p:0", "f:0")).toBeUndefined()
    await loadImagePage(client, faces)
    expect(read).toHaveBeenCalledOnce()
  })
  it("does not background-download anything without persistent storage", async () => {
    let requests = 0
    const bytes = new MetadataBytes(
      "/cdn",
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
  it("removes legacy metadata namespaces for this root while retaining shared raw across versions", async () => {
    const cache = storage()
    await cache.open("unrelated-app")
    await cache.open("sve-images-%2Fcdn-preview:-preview")
    await cache.open("sve-images-%2Fcdn:-a")
    await cache.open("sve-images-%2Fcdn:-b")
    const fetcher: Fetcher = () =>
      Promise.resolve(new Response(canonical({ synthetic: 0 }).slice().buffer))
    const first = new MetadataBytes("/cdn", metadataFiles(1), fetcher, () => undefined, cache)
    await first.prefetch()
    const second = new MetadataBytes(
      "/cdn",
      metadataFiles(1),
      fetcher,
      () => undefined,
      cache,
      fetcher,
    )
    await second.prefetch()
    expect((await cache.keys()).sort()).toEqual(
      ["sve-raw:%2Fcdn", "sve-images-%2Fcdn-preview:-preview", "unrelated-app"].sort(),
    )
    const third = new MetadataBytes(
      "/cdn",
      metadataFiles(1),
      fetcher,
      () => undefined,
      cache,
      fetcher,
    )
    await third.prefetch()
    expect((await cache.keys()).sort()).toEqual(
      ["sve-raw:%2Fcdn", "sve-images-%2Fcdn-preview:-preview", "unrelated-app"].sort(),
    )
  })
  it("revalidates equal-length persistent bytes instead of accepting a corrupt cache entry", async () => {
    const cache = storage()
    const opened = await cache.open("sve-raw:%2Fcdn")
    const hash = digest(canonical({ synthetic: 0 }))
    await opened.put(
      `/cdn/__sve-raw/${encodeURIComponent(hash)}/bytes`,
      new Response(canonical({ synthetic: 9 }).slice().buffer),
    )
    let requests = 0
    const bytes = new MetadataBytes(
      "/cdn",
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
    const bytes = new Map(source)
    const served = serve(bytes)
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
    // Publish a new manifest; a reload that finds the same one keeps the snapshot instead.
    const next = canonical({ ...manifest, data_version: "preview-20991231T000000Z-0001" })
    const nextPath = `snapshots/manifests/${digest(next).slice(7)}.json`
    bytes.set(nextPath, next)
    bytes.set(
      "snapshots/preview/current.json",
      canonical({ manifest_path: nextPath, manifest_sha256: digest(next) }),
    )
    await client.reload()
    await prefetch
    expect(aborted).toBeGreaterThan(0)
  })
  it("reopens persistent bytes across sessions and manifests without retaining a raw memory LRU", async () => {
    const files = metadataFiles(70)
    const requests: string[] = []
    const fetcher: Fetcher = (url) => {
      requests.push(url)
      const i = Number(url.split("data")[1])
      return Promise.resolve(new Response(canonical({ synthetic: i }).slice().buffer))
    }
    const sharedCache = storage()
    const bytes = new MetadataBytes("/cdn", files, fetcher, () => undefined, sharedCache)
    await bytes.prefetch()
    expect(requests).toHaveLength(70)
    await bytes.read("file0")
    expect(requests).toHaveLength(70)
    const reopened = new MetadataBytes("/cdn", files, fetcher, () => undefined, sharedCache)
    await reopened.read("file0")
    expect(requests).toHaveLength(70)
    const separate = new MetadataBytes("/cdn", files, fetcher, () => undefined, sharedCache)
    await separate.read("file0")
    expect(requests).toHaveLength(70)
    expect(bytes.status()).toMatchObject({ state: "complete", done: 70 })
    await sharedCache.delete("sve-raw:%2Fcdn")
    await bytes.read("file0")
    expect(requests).toHaveLength(71)
    // One restored file cannot vouch for the other 69 that the clear took away.
    expect(bytes.status()).toMatchObject({ persistent: true, state: "cancelled", done: 1 })
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
    const bytes = new MetadataBytes("/cdn", files, served, () => undefined, storage(true))
    await bytes.prefetch()
    expect(bytes.status()).toMatchObject({ persistent: false, state: "cancelled" })
    expect(requests).toBeLessThanOrEqual(3)
    expect(await bytes.read("file0")).toEqual(canonical({ synthetic: 0 }))
    const after = requests
    expect(await bytes.read("file0")).toEqual(canonical({ synthetic: 0 }))
    expect(requests).toBe(after + 1)
    expect(bytes.status()).toMatchObject({ persistent: false, state: "cancelled", done: 0 })
  })
  it("rejects a replaced snapshot even when its transport ignores abort", async () => {
    let finish: ((response: Response) => void) | undefined
    const bytes = new MetadataBytes(
      "/cdn",
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
  it("keeps slots free for the page while background bodies are stalled", async () => {
    const starts: string[] = []
    const release: (() => void)[] = []
    const queue = requestQueue(async (url) => {
      starts.push(url)
      await new Promise<void>((resolve) => {
        release.push(resolve)
      })
      return new Response("{}")
    })
    const background = Array.from({ length: 27 }, (_, i) =>
      queue.background(`background${String(i)}`),
    )
    await Promise.resolve()
    expect(starts).toHaveLength(24)
    const visible = queue.foreground("visible")
    expect(starts).toEqual([
      ...Array.from({ length: 24 }, (_, i) => `background${String(i)}`),
      "visible",
    ])
    while (starts.length < 28 || release.length > 0) {
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
