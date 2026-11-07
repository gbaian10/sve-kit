import { describe, expect, it } from "vitest"

import { memoryCache } from "../test-utils/cache"
import {
  fakeScanCdn,
  fullCache,
  RUNTIME,
  SCAN_BASE,
  scanPublication,
} from "../test-utils/scan-publication"
import { createScanModelStore, SCAN_PARTS, ScanModelError } from "./scan-models"

function setup(options: { cdn?: ReturnType<typeof fakeScanCdn>; storage?: CacheStorage } = {}) {
  const cdn = options.cdn ?? fakeScanCdn()
  const storage = options.storage ?? memoryCache()
  const store = createScanModelStore({
    caches: () => storage,
    fetcher: cdn.fetcher,
    scanBase: () => SCAN_BASE,
    runtime: RUNTIME,
  })
  return { cdn, store }
}

async function downloadAll(store: ReturnType<typeof setup>["store"], edition: "jp" | "en" = "jp") {
  for (const part of SCAN_PARTS) await store.download(part, edition, () => undefined)
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
    expect(status.parts.runtime).toEqual({ state: "missing", bytes: 64, missingBytes: 64 })
    expect(status.parts.det).toMatchObject({ state: "missing", version: 1 })
    expect(status.parts.rec).toMatchObject({
      state: "missing",
      version: 1,
      cards: 5,
      dataVersion: "2026-09-29",
    })
    expect(status.parts.rec.missingBytes).toBe(status.parts.rec.bytes)
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
    await store.download("runtime", "jp", () => undefined)

    const status = await store.status("jp")
    expect(SCAN_PARTS.map((part) => status.parts[part].state)).toEqual(["ready", "ready", "ready"])
    expect(status.storedBytes).toBeGreaterThan(status.parts.rec.bytes)

    cdn.setOffline(true)
    const models = await store.load("jp")
    expect(models.runtime.length).toBe(RUNTIME.bytes)
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

  it("does not hand an older site's runtime to this site's ONNX Runtime", async () => {
    const cdn = fakeScanCdn()
    const storage = memoryCache()
    const { store: older } = setup({ cdn, storage })
    for (const part of SCAN_PARTS) await older.download(part, "jp", () => undefined)
    const newer = createScanModelStore({
      caches: () => storage,
      fetcher: cdn.fetcher,
      scanBase: () => SCAN_BASE,
      runtime: { url: "https://app.test/assets/ort-next.wasm", bytes: RUNTIME.bytes },
    })
    expect((await newer.status("jp")).parts.runtime.state).toBe("missing")
    await expect(newer.load("jp")).rejects.toThrow(ScanModelError)
  })

  it("reports a full device as a quota failure", async () => {
    const { store } = setup({ storage: fullCache() })
    expect(await failure(store.download("det", "jp", () => undefined))).toBe("quota")
    expect(await failure(store.download("runtime", "jp", () => undefined))).toBe("quota")
  })

  it("reports a lost connection as a network failure", async () => {
    const cdn = fakeScanCdn()
    const { store } = setup({ cdn })
    await store.status("jp")
    cdn.setOffline(true)
    expect(await failure(store.download("runtime", "jp", () => undefined))).toBe("network")
    expect(await failure(store.download("det", "jp", () => undefined))).toBe("network")
  })

  it("deletes one part or everything", async () => {
    const { store } = setup()
    await downloadAll(store)
    await store.remove("det")
    let status = await store.status("jp")
    expect(status.parts.det.state).toBe("missing")
    expect(status.parts.rec.state).toBe("ready")
    await expect(store.load("jp")).rejects.toThrow(ScanModelError)

    await store.remove("rec")
    status = await store.status("jp")
    expect(status.parts.rec.state).toBe("missing")
    expect(status.storedBytes).toBe(RUNTIME.bytes)

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
    expect((await store.load("jp")).rec.model.version).toBe(1)

    const before = status.storedBytes
    await store.download("rec", "jp", () => undefined)
    const after = await store.status("jp")
    expect(after.parts.rec.state).toBe("ready")
    expect((await store.load("jp")).rec.model.version).toBe(2)
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
    expect((await store.load("en")).index.cards[0]?.name).toBe("en BP01-001")
    expect((await store.load("jp")).index.cards[0]?.name).toBe("jp BP01-001")
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
