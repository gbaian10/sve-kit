import { afterEach, describe, expect, it, vi } from "vitest"

import { memoryCache } from "../test-utils/cache"
import {
  fakeScanCdn,
  fullCache,
  RUNTIMES,
  SCAN_BASE,
  scanPublication,
} from "../test-utils/scan-publication"
import { createScanModelStore, ScanModelError, type ScanPart } from "./scan-models"

const CPU: ScanPart = "runtime-cpu"
const WEBGPU: ScanPart = "runtime-webgpu"
// The parts a scan with the CPU engine needs.
const CPU_PARTS: readonly ScanPart[] = [CPU, "det", "rec"]

afterEach(() => vi.restoreAllMocks())

function setup(options: { cdn?: ReturnType<typeof fakeScanCdn>; storage?: CacheStorage } = {}) {
  const cdn = options.cdn ?? fakeScanCdn()
  const storage = options.storage ?? memoryCache()
  const store = createScanModelStore({
    caches: () => storage,
    fetcher: cdn.fetcher,
    scanBase: () => SCAN_BASE,
    runtimes: RUNTIMES,
  })
  return { cdn, store }
}

async function downloadAll(store: ReturnType<typeof setup>["store"], edition: "jp" | "en" = "jp") {
  for (const part of CPU_PARTS) await store.download(part, edition, () => undefined)
}

async function failure(promise: Promise<unknown>): Promise<string> {
  try {
    await promise
  } catch (error) {
    if (error instanceof ScanModelError) return error.kind
    throw error
  }
  throw new Error("expected a failure")
}

describe("scan model store", () => {
  it("lists every part as missing with its published size before anything downloads", async () => {
    const { store } = setup()
    const status = await store.status("jp")
    expect(status.manifest).toBe("ok")
    expect(status.storedBytes).toBe(0)
    expect(status.parts[CPU]).toEqual({
      state: "missing",
      bytes: RUNTIMES.cpu.bytes,
      missingBytes: RUNTIMES.cpu.bytes,
    })
    expect(status.parts[WEBGPU]).toEqual({
      state: "missing",
      bytes: RUNTIMES.webgpu.bytes,
      missingBytes: RUNTIMES.webgpu.bytes,
    })
    expect(status.parts.det).toMatchObject({ state: "missing", version: 1 })
    expect(status.parts.rec).toMatchObject({
      state: "missing",
      version: 1,
      cards: 5,
      dataVersion: "2026-09-29",
    })
    expect(status.parts.rec.missingBytes).toBe(status.parts.rec.bytes)
    // The index is part of the recogniser's size; the model takes the rest.
    expect(status.parts.rec.indexBytes).toBeGreaterThan(0)
    expect(status.parts.rec.indexBytes).toBeLessThan(status.parts.rec.bytes)
  })

  it("downloads each part with progress up to its fixed total, then reads it back from the device", async () => {
    const { cdn, store } = setup()
    const progress: [number, number][] = []
    await store.download("rec", "jp", (got, total) => progress.push([got, total]))
    const totals = new Set(progress.map(([, total]) => total))
    expect(totals.size).toBe(1)
    expect(progress.at(-1)?.[0]).toBe(progress.at(-1)?.[1])
    expect(progress.map(([got]) => got)).toEqual(
      [...progress.map(([got]) => got)].sort((a, b) => a - b),
    )
    await store.download("det", "jp", () => undefined)
    await store.download(CPU, "jp", () => undefined)

    const status = await store.status("jp")
    expect(CPU_PARTS.map((part) => status.parts[part].state)).toEqual(["ready", "ready", "ready"])
    expect(status.parts[WEBGPU].state).toBe("missing")
    expect(status.storedBytes).toBeGreaterThan(status.parts.rec.bytes)

    cdn.setOffline(true)
    const models = await store.load("jp", "cpu")
    expect(models.backend).toBe("cpu")
    expect(models.runtime.length).toBe(RUNTIMES.cpu.bytes)
    expect(new TextDecoder().decode(models.rec.bytes)).toBe("rec model 1")
    expect(Array.from(models.index.vectors.subarray(8, 12))).toEqual([0, 127, 0, 0])
    expect(models.index.scale[0]).toBeCloseTo(1 / 127)
    expect(models.index.hub[4]).toBeCloseTo(0.5)
    expect(models.index.cards.map((card) => card.no)).toContain("BP01-T01")
  })

  it("rejects a file whose SHA-256 does not match and keeps nothing of it", async () => {
    const cdn = fakeScanCdn()
    const { store } = setup({ cdn })
    const recFile = scanPublication().manifest.rec.file
    cdn.corrupt(recFile)
    expect(await failure(store.download("rec", "jp", () => undefined))).toBe("integrity")
    const status = await store.status("jp")
    expect(status.parts.rec.state).toBe("missing")
    expect(status.storedBytes).toBe(0)
  })

  it("cleans up a recogniser download that fails part way, so nothing is stranded", async () => {
    const cdn = fakeScanCdn()
    const { store } = setup({ cdn })
    await store.download("det", "jp", () => undefined)
    const before = (await store.status("jp")).storedBytes
    cdn.corrupt(scanPublication().manifest.index.jp.vectors.file)
    expect(await failure(store.download("rec", "jp", () => undefined))).toBe("integrity")
    const status = await store.status("jp")
    expect(status.storedBytes).toBe(before)
    expect(status.parts.det.state).toBe("ready")
  })

  it("does not hand the runtime to this build unless its hash is the pinned one", async () => {
    const cdn = fakeScanCdn()
    const storage = memoryCache()
    const { store: older } = setup({ cdn, storage })
    await downloadAll(older)
    const newer = createScanModelStore({
      caches: () => storage,
      fetcher: cdn.fetcher,
      scanBase: () => SCAN_BASE,
      runtimes: { ...RUNTIMES, cpu: { ...RUNTIMES.cpu, sha256: "0".repeat(64) } },
    })
    expect((await newer.status("jp")).parts[CPU].state).toBe("update")
    await expect(newer.load("jp", "cpu")).rejects.toThrow(ScanModelError)
  })

  it("downloads from the second CDN when the first cannot be reached", async () => {
    const cdn = fakeScanCdn()
    const { store } = setup({ cdn })
    cdn.unreachable(RUNTIMES.cpu.urls[0] ?? "")
    await store.download(CPU, "jp", () => undefined)
    expect(cdn.requests.filter((url) => RUNTIMES.cpu.urls.includes(url))).toEqual(RUNTIMES.cpu.urls)
    expect((await store.status("jp")).parts[CPU].state).toBe("ready")
  })

  it("preserves the installed runtime if saving an update's install record fails", async () => {
    const storage = memoryCache()
    let failRecord = false
    const open = storage.open.bind(storage)
    vi.spyOn(storage, "open").mockImplementation(async (name) => {
      const cache = await open(name)
      const put = cache.put.bind(cache)
      vi.spyOn(cache, "put").mockImplementation(async (...args) => {
        if (
          failRecord &&
          (await args[1].clone().arrayBuffer()).byteLength !== RUNTIMES.webgpu.bytes
        )
          throw new DOMException("full", "QuotaExceededError")
        await put(...args)
      })
      return cache
    })
    const cdn = fakeScanCdn()
    const { store: older } = setup({ cdn, storage })
    await downloadAll(older)
    const before = await older.load("jp", "cpu")
    const stored = (await older.status("jp")).storedBytes
    const newer = createScanModelStore({
      caches: () => storage,
      fetcher: cdn.fetcher,
      scanBase: () => SCAN_BASE,
      runtimes: { ...RUNTIMES, cpu: RUNTIMES.webgpu },
    })
    failRecord = true
    expect(await failure(newer.download(CPU, "jp", () => undefined))).toBe("quota")
    expect((await older.load("jp", "cpu")).runtime).toEqual(before.runtime)
    expect((await newer.status("jp")).storedBytes).toBe(stored)
    failRecord = false
    await newer.download(CPU, "jp", () => undefined)
    expect((await newer.load("jp", "cpu")).runtime.length).toBe(RUNTIMES.webgpu.bytes)
    expect((await newer.status("jp")).storedBytes).toBe(
      stored - RUNTIMES.cpu.bytes + RUNTIMES.webgpu.bytes,
    )
  })

  it("refuses a runtime with the wrong size without switching CDNs", async () => {
    const cdn = fakeScanCdn()
    const store = createScanModelStore({
      caches: () => memoryCache(),
      fetcher: cdn.fetcher,
      scanBase: () => SCAN_BASE,
      runtimes: { ...RUNTIMES, cpu: { ...RUNTIMES.cpu, bytes: RUNTIMES.cpu.bytes + 1 } },
    })
    expect(await failure(store.download(CPU, "jp", () => undefined))).toBe("integrity")
    expect(cdn.requests.filter((url) => RUNTIMES.cpu.urls.includes(url))).toHaveLength(1)
  })

  it("refuses a runtime whose bytes do not match the pinned hash, from any CDN", async () => {
    const cdn = fakeScanCdn()
    const { store } = setup({ cdn })
    cdn.corruptRuntime("cpu")
    expect(await failure(store.download(CPU, "jp", () => undefined))).toBe("integrity")
    const status = await store.status("jp")
    expect(status.parts[CPU].state).toBe("missing")
    expect(status.storedBytes).toBe(0)
    // A bad file is not a reason to try another host: the mismatch is final.
    expect(cdn.requests.filter((url) => RUNTIMES.cpu.urls.includes(url))).toHaveLength(1)
  })

  it("keeps the two runtimes apart, so switching engines downloads only the other one", async () => {
    const { store } = setup()
    await store.download("det", "jp", () => undefined)
    await store.download("rec", "jp", () => undefined)
    await store.download(CPU, "jp", () => undefined)
    await store.download(WEBGPU, "jp", () => undefined)
    const status = await store.status("jp")
    expect(status.parts[CPU].state).toBe("ready")
    expect(status.parts[WEBGPU].state).toBe("ready")
    expect((await store.load("jp", "webgpu")).runtime.length).toBe(RUNTIMES.webgpu.bytes)
    await store.remove(WEBGPU)
    expect((await store.status("jp")).parts[CPU].state).toBe("ready")
    await expect(store.load("jp", "webgpu")).rejects.toThrow(ScanModelError)
  })

  it("reports a full device as a quota failure", async () => {
    const { store } = setup({ storage: fullCache() })
    expect(await failure(store.download("det", "jp", () => undefined))).toBe("quota")
    expect(await failure(store.download(CPU, "jp", () => undefined))).toBe("quota")
  })

  it("reports a lost connection as a network failure", async () => {
    const cdn = fakeScanCdn()
    const { store } = setup({ cdn })
    await store.status("jp")
    cdn.setOffline(true)
    expect(await failure(store.download(CPU, "jp", () => undefined))).toBe("network")
    expect(await failure(store.download("det", "jp", () => undefined))).toBe("network")
  })

  it("deletes one part or everything", async () => {
    const { store } = setup()
    await downloadAll(store)
    await store.remove("det")
    let status = await store.status("jp")
    expect(status.parts.det.state).toBe("missing")
    expect(status.parts.rec.state).toBe("ready")
    await expect(store.load("jp", "cpu")).rejects.toThrow(ScanModelError)

    await store.remove("rec")
    status = await store.status("jp")
    expect(status.parts.rec.state).toBe("missing")
    expect(status.storedBytes).toBe(RUNTIMES.cpu.bytes)

    await store.removeAll()
    expect((await store.status("jp")).storedBytes).toBe(0)
  })

  it("keeps scanning on the installed copy until the user updates to a newer publication", async () => {
    const cdn = fakeScanCdn()
    const { store } = setup({ cdn })
    await downloadAll(store)
    cdn.publish(scanPublication({ rec: 2, dataVersion: "2026-11-01" }))

    const status = await store.status("jp")
    expect(status.parts.rec).toMatchObject({
      state: "update",
      version: 2,
      dataVersion: "2026-11-01",
    })
    expect(status.parts.det.state).toBe("ready")
    expect((await store.load("jp", "cpu")).rec.model.version).toBe(1)

    const before = status.storedBytes
    await store.download("rec", "jp", () => undefined)
    const after = await store.status("jp")
    expect(after.parts.rec.state).toBe("ready")
    expect((await store.load("jp", "cpu")).rec.model.version).toBe(2)
    // The old recogniser and its index are gone, not kept beside the new one.
    expect(after.storedBytes).toBeLessThan(before + after.parts.rec.bytes)
  })

  it("is not ready until the part's install record names the published files", async () => {
    const cdn = fakeScanCdn()
    const storage = memoryCache()
    const { store } = setup({ cdn, storage })
    await store.download("det", "jp", () => undefined)
    await (await storage.open("sve-scan-models-v1")).delete("/scan-models/installed/det.json")
    const status = await store.status("jp")
    expect(status.parts.det).toMatchObject({ state: "missing", missingBytes: 0 })
    cdn.requests.length = 0
    await store.download("det", "jp", () => undefined)
    expect(cdn.requests.filter((url) => url.includes("/models/"))).toEqual([])
    expect((await store.status("jp")).parts.det.state).toBe("ready")
  })

  it("adds the other edition's index without downloading the recogniser again", async () => {
    const cdn = fakeScanCdn()
    const { store } = setup({ cdn })
    await downloadAll(store)
    const en = await store.status("en")
    expect(en.parts.rec.state).toBe("missing")
    expect(en.parts.rec.missingBytes).toBeLessThan(en.parts.rec.bytes)

    cdn.requests.length = 0
    await store.download("rec", "en", () => undefined)
    expect(cdn.requests.filter((url) => url.includes("/models/"))).toEqual([])
    expect((await store.load("en", "cpu")).index.cards[0]?.name).toBe("en BP01-001")
    expect((await store.load("jp", "cpu")).index.cards[0]?.name).toBe("jp BP01-001")
  })

  it("says when an edition has no published index", async () => {
    const { store } = setup({ cdn: fakeScanCdn(scanPublication({ en: null })) })
    expect((await store.status("en")).parts.rec).toEqual({
      state: "missing",
      bytes: 0,
      missingBytes: 0,
    })
    expect(await failure(store.download("rec", "en", () => undefined))).toBe("missing")
  })

  it("works from installed copies when the list cannot be read", async () => {
    const cdn = fakeScanCdn()
    const { store } = setup({ cdn })
    await downloadAll(store)
    cdn.setOffline(true)
    const status = await store.status("jp")
    expect(status.manifest).toBe("offline")
    expect(status.parts.rec).toMatchObject({ state: "ready", version: 1 })
    expect(await failure(store.download("det", "jp", () => undefined))).toBe("network")
  })

  it("asks for a site update when the list uses a format this site does not know", async () => {
    const { store } = setup({ cdn: fakeScanCdn(scanPublication({ format: 2 })) })
    expect((await store.status("jp")).manifest).toBe("unsupported")
    expect(await failure(store.download("det", "jp", () => undefined))).toBe("unsupported")
  })
})
