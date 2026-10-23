import { brotliCompressSync } from "node:zlib"

import { act, waitFor } from "@testing-library/react"
import { afterEach, expect, it, vi } from "vitest"

import { buildSnapshot } from "../../scripts/fixture/build"
import { memoryCache } from "../test-utils/cache"
import { BackgroundDownloads } from "./background-downloads"
import { bodyHeaders, dataCacheBytes } from "./cache-budget"
import type { Fetcher } from "./cdn"
import type { FetchSelection } from "./fetch-plan"
import {
  arrayValue,
  canonicalText,
  type JsonObject,
  objectValue,
  stringValue,
} from "./format-v3/json"
import { transferDigest } from "./integrity"
import { rawBlobDescriptor, RawBlobStore } from "./raw-blobs"

const base = "https://lifecycle.test/cdn"

afterEach(() => {
  vi.restoreAllMocks()
})

async function fixture() {
  const files = new Map<string, JsonObject>()
  const bodies = new Map<string, Uint8Array>()
  for (const key of ["saved", "held", "queued"]) {
    const bytes = new TextEncoder().encode(`bytes ${key}`)
    const file = {
      key,
      path: `${key}.json`,
      sha256: await transferDigest(bytes),
      bytes: bytes.length,
      compressed_bytes: { br: bytes.length - 1 },
      role: "text",
      region: "jp",
      dependencies: [],
      source_refs: [],
    }
    files.set(key, file)
    bodies.set(`${base}/${file.path}`, bytes)
  }
  const selection: FetchSelection = {
    base,
    dataVersion: "v1",
    manifestHash: "m1",
    edition: "jp",
    files,
  }
  return { selection, bodies }
}

it("pauses while hidden or offline and resumes only missing files when both conditions clear", async () => {
  for (const clear of [false, true]) {
    let hidden = false
    let online = true
    vi.spyOn(document, "hidden", "get").mockImplementation(() => hidden)
    vi.spyOn(navigator, "onLine", "get").mockImplementation(() => online)
    const data = await fixture()
    const gate = Promise.withResolvers<undefined>()
    const network = vi.fn<Fetcher>(async (url) => {
      if (!url.endsWith("saved.json")) await gate.promise
      const bytes = data.bodies.get(url)
      return new Response(bytes?.slice().buffer)
    })
    const storage = memoryCache()
    const downloads = new BackgroundDownloads({
      enabled: true,
      storage,
      foreground: network,
      background: network,
    })
    try {
      downloads.select(data.selection)
      await waitFor(() => {
        expect(downloads.getStatus().progress?.missing).toBe(2)
      })
      act(() => {
        hidden = true
        document.dispatchEvent(new Event("visibilitychange"))
      })
      expect(downloads.getStatus().state).toBe("paused")
      const paused = downloads.getStatus()
      gate.resolve(undefined)
      await new Promise((resolve) => setTimeout(resolve, 0))
      expect(downloads.getStatus()).toBe(paused)
      act(() => {
        online = false
        window.dispatchEvent(new Event("offline"))
      })
      act(() => {
        hidden = false
        document.dispatchEvent(new Event("visibilitychange"))
      })
      expect(downloads.getStatus().state).toBe("paused")
      if (clear) await storage.delete(`sve-raw:${encodeURIComponent(base)}`)
      act(() => {
        online = true
        window.dispatchEvent(new Event("online"))
      })
      await waitFor(() => {
        expect(downloads.getStatus()).toMatchObject({
          state: "settled",
          progress: { phase: "complete", missing: 0 },
        })
      })
      expect(network.mock.calls.filter(([url]) => url.endsWith("saved.json"))).toHaveLength(
        clear ? 2 : 1,
      )
      downloads.setEnabled(false)
      expect(downloads.getStatus().state).toBe("disabled")
    } finally {
      downloads.dispose()
      vi.restoreAllMocks()
    }
  }
})

it("cancels replaced identities and ignores late writes from each previous root, version, manifest or edition", async () => {
  vi.spyOn(document, "hidden", "get").mockReturnValue(false)
  vi.spyOn(navigator, "onLine", "get").mockReturnValue(true)
  for (const change of [
    { base: `${base}/other` },
    { dataVersion: "v2" },
    { manifestHash: "m2" },
    { edition: "en" as const },
  ]) {
    const data = await fixture()
    const file = data.selection.files.get("saved")
    if (!file) throw new Error("missing file")
    const storage = memoryCache()
    const name = `sve-raw:${encodeURIComponent(base)}`
    const cache = await storage.open(name)
    const open = storage.open.bind(storage)
    storage.open = (key) => (key === name ? Promise.resolve(cache) : open(key))
    const entered = Promise.withResolvers<undefined>()
    const gate = Promise.withResolvers<undefined>()
    const written = Promise.withResolvers<undefined>()
    const put = cache.put.bind(cache)
    let hold = true
    cache.put = async (key, response) => {
      const blocked = hold
      if (hold) {
        hold = false
        entered.resolve(undefined)
        await gate.promise
      }
      await put(key, response)
      if (blocked) written.resolve(undefined)
    }
    const network = vi.fn<Fetcher>(() =>
      Promise.resolve(new Response(data.bodies.get(`${base}/saved.json`)?.slice().buffer)),
    )
    const added = [vi.spyOn(document, "addEventListener"), vi.spyOn(window, "addEventListener")]
    const removed = [
      vi.spyOn(document, "removeEventListener"),
      vi.spyOn(window, "removeEventListener"),
    ]
    for (const spy of [...added, ...removed]) spy.mockClear()
    const downloads = new BackgroundDownloads({ storage, foreground: network, background: network })
    const updates = vi.fn()
    downloads.subscribe(updates)
    try {
      downloads.select({ ...data.selection, files: new Map([["saved", file]]) })
      await new Promise((resolve) => setTimeout(resolve, 0))
      expect(downloads.getStatus().state).toBe("disabled")
      expect(network).not.toHaveBeenCalled()
      downloads.setEnabled(true)
      await entered.promise
      const next = {
        ...data.selection,
        ...change,
        files: new Map([["saved", { ...file, region: change.edition ?? "jp" }]]),
      }
      downloads.select(next)
      await waitFor(() => {
        expect(downloads.getStatus()).toMatchObject({
          state: "settled",
          progress: { phase: "complete" },
        })
      })
      const settled = downloads.getStatus()
      const published = updates.mock.calls.length
      gate.resolve(undefined)
      await written.promise
      await new Promise((resolve) => setTimeout(resolve, 0))
      expect(downloads.getStatus()).toBe(settled)
      expect(updates).toHaveBeenCalledTimes(published)
      expect(downloads.getStatus().selection).toMatchObject(change)
      downloads.dispose()
      // The disposed guard alone would hide a leaked browser listener.
      const lifecycle = ["visibilitychange", "online", "offline"]
      const listeners = added.map((spy) =>
        spy.mock.calls.filter(([type]) => lifecycle.includes(type)),
      )
      expect(listeners.flat()).toHaveLength(3)
      for (const [index, calls] of listeners.entries())
        for (const [type, listener] of calls)
          expect(removed[index]).toHaveBeenCalledWith(type, listener)
      const disposed = downloads.getStatus()
      window.dispatchEvent(new Event("offline"))
      expect(downloads.getStatus()).toBe(disposed)
    } finally {
      gate.resolve(undefined)
      downloads.dispose()
    }
  }
})

it("admits the whole compressed batch including saved startup bytes and the manifest, and preserves old data on capacity failure", async () => {
  vi.spyOn(document, "hidden", "get").mockReturnValue(false)
  vi.spyOn(navigator, "onLine", "get").mockReturnValue(true)
  for (const mode of ["limit", "unknown", "capacity", "deduplicated"] as const) {
    const data = await fixture()
    const saved = data.selection.files.get("saved")
    if (!saved) throw new Error("fixture missing")
    const network = vi.fn<Fetcher>((url) =>
      Promise.resolve(new Response(data.bodies.get(url)?.slice().buffer)),
    )
    const storage = memoryCache()
    const raw = new RawBlobStore(base, network, storage)
    await raw.read(rawBlobDescriptor(saved))
    network.mockClear()
    const cache = await storage.open(`sve-search-index:1:${encodeURIComponent(base)}`)
    const oldHash = `sha256:${"1".repeat(64)}`
    const oldSlot = `${base}/old-active/${encodeURIComponent(oldHash)}`
    if (mode === "capacity" || mode === "deduplicated") {
      await raw.retain(data.selection.files, oldHash)
      if (mode === "deduplicated")
        await raw.retain(new Map([["saved", saved]]), data.selection.manifestHash)
      const occupied = mode === "deduplicated" ? await dataCacheBytes(storage) : 0
      await cache.put(
        oldSlot,
        new Response("old", { headers: bodyHeaders(96 * 1024 * 1024 - occupied) }),
      )
    }
    const downloads = new BackgroundDownloads({
      enabled: true,
      storage,
      foreground: network,
      background: network,
    })
    try {
      downloads.select(
        mode === "deduplicated"
          ? { ...data.selection, files: new Map([["saved", saved]]) }
          : {
              ...data.selection,
              files: new Map([
                [
                  "saved",
                  {
                    ...saved,
                    compressed_bytes: {
                      br: mode === "unknown" ? null : mode === "limit" ? 12 * 1024 * 1024 : 1,
                    },
                  },
                ],
              ]),
              manifest: {
                ...rawBlobDescriptor(saved),
                key: "manifest",
                sha256: "sha256:manifest",
                bytes: 1,
                brotli: 1,
              },
            },
      )
      await waitFor(() => {
        expect(downloads.getStatus().state).toBe("settled")
      })
      expect(downloads.getStatus().progress).toMatchObject(
        mode === "deduplicated"
          ? { phase: "complete", missing: 0 }
          : {
              phase: "partial",
              reason: mode === "capacity" ? "capacity" : "batch-limit",
              missing: 1,
            },
      )
      expect(network).not.toHaveBeenCalled()
      if (mode === "capacity" || mode === "deduplicated")
        expect(await cache.match(oldSlot)).toBeDefined()
    } finally {
      downloads.dispose()
    }
  }
})

it("uses only the adopted exact manifest after restore, includes it in the byte plan, and cancels metadata on selection changes", async () => {
  vi.spyOn(document, "hidden", "get").mockReturnValue(false)
  vi.spyOn(navigator, "onLine", "get").mockReturnValue(true)
  const built = await buildSnapshot({ encodeImage: () => Promise.resolve(new Uint8Array([1])) })
  for (const value of arrayValue(built.manifest["files"])) {
    const file = objectValue(value)
    const body = built.files.get(stringValue(file["path"]))
    if (!body) throw new Error("file missing")
    file["compressed_bytes"] = { br: brotliCompressSync(body).byteLength }
  }
  const bytes = new TextEncoder().encode(canonicalText(built.manifest))
  built.files.set(built.manifestPath, bytes)
  const network = vi.fn<Fetcher>((url) => {
    const body = built.files.get(url.slice(base.length + 1))
    return Promise.resolve(
      body
        ? new Response(body.slice().buffer, {
            headers: { "content-encoding": "br", "content-length": "100" },
          })
        : new Response(null, { status: 404 }),
    )
  })
  const storage = memoryCache()
  const downloads = new BackgroundDownloads({
    storage,
    foreground: network,
    background: network,
  })
  const adopted = {
    base,
    manifestPath: built.manifestPath,
    manifestHash: await transferDigest(bytes),
    dataVersion: stringValue(built.manifest["data_version"]),
    edition: "en" as const,
  }
  try {
    downloads.adopt(adopted)
    await waitFor(() => {
      expect(downloads.getStatus().state).toBe("disabled")
    })
    downloads.setEnabled(true)
    await waitFor(() => {
      expect(downloads.getStatus().progress?.phase).toBe("complete")
    })
    expect(
      network.mock.calls.every(
        ([url]) => !url.endsWith("current.json") && !url.endsWith("index.json"),
      ),
    ).toBe(true)
    expect(network.mock.calls.some(([url]) => url === `${base}/${built.manifestPath}`)).toBe(true)
    downloads.preferEdition(base, "jp")
    expect(downloads.getStatus().selection?.edition).toBe("jp")
    await waitFor(() => {
      expect(downloads.getStatus().progress?.phase).toBe("complete")
    })
    await storage.delete(`sve-raw:${encodeURIComponent(base)}`)
    await downloads.recheck()
    expect(downloads.getStatus().progress).toMatchObject({ phase: "partial", reason: "cleared" })
    const held = Promise.withResolvers<Response>()
    network.mockImplementation(() => held.promise)
    downloads.adopt({ ...adopted, base: `${base}/next` })
    const signal = network.mock.calls.at(-1)?.[1]?.signal
    downloads.adopt(null)
    expect(signal?.aborted).toBe(true)
    const status = downloads.getStatus()
    held.resolve(new Response(bytes.slice().buffer))
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(downloads.getStatus()).toBe(status)
  } finally {
    downloads.dispose()
  }
})
