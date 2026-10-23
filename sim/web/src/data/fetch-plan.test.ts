// @vitest-environment node
import { expect, it, vi } from "vitest"

import { memoryCache } from "../test-utils/cache"
import type { Fetcher } from "./cdn"
import { FetchPlan, type FetchProgress } from "./fetch-plan"
import type { JsonObject } from "./format-v3/json"
import { transferDigest } from "./integrity"
import { rawBlobDescriptor, RawBlobStore } from "./raw-blobs"

const base = "https://plan.test/cdn"

async function fixture() {
  const files = new Map<string, JsonObject>()
  const bodies = new Map<string, Uint8Array>()
  const add = async (key: string, role: string, region: string | null): Promise<JsonObject> => {
    // Deliberately invalid JSON: background downloads must never decode containers.
    const bytes = new TextEncoder().encode(`bytes ${key}`)
    const file = {
      key,
      role,
      region,
      path: `${key}.json`,
      sha256: await transferDigest(bytes),
      bytes: bytes.length,
      compressed_bytes: { br: bytes.length - 1 },
      dependencies: [],
      source_refs: [],
    }
    files.set(key, file)
    bodies.set(`${base}/${file.path}`, bytes)
    return file
  }
  const common = await add("common", "config", null)
  const jp = await add("jp", "text", "jp")
  const en = await add("en", "text", "en")
  const source = await add("source", "text", "jp")
  const originalBase = await add("old-base", "bootstrap", "jp")
  const changedBase = await add("changed-base", "bootstrap", "jp")
  const recursive = await add("recursive", "text", "jp")
  const media = await add("media", "images", null)
  const programs = await add("programs", "programs", null)
  const ref = (file: JsonObject) => ({ key: file["key"] ?? null, sha256: file["sha256"] ?? null })
  en["source_refs"] = [ref(source)]
  source["dependencies"] = [ref(changedBase)]
  source["source_refs"] = [ref(recursive)]
  media["dependencies"] = [ref(jp)]
  const network = vi.fn<Fetcher>((url) => {
    const body = bodies.get(url)
    return Promise.resolve(
      body ? new Response(body.slice().buffer) : new Response(null, { status: 404 }),
    )
  })
  return {
    files,
    bodies,
    common,
    jp,
    en,
    source,
    originalBase,
    changedBase,
    recursive,
    media,
    programs,
    network,
  }
}

it("fixes each edition's one-hop text closure, exact changed base and byte totals before downloading", async () => {
  const data = await fixture()
  const storage = memoryCache()
  const raw = new RawBlobStore(base, data.network, storage, data.network)
  await raw.read(rawBlobDescriptor(data.common))
  data.network.mockClear()
  const plan = new FetchPlan(
    { base, manifestHash: "adopted", dataVersion: "v1", edition: "en", files: data.files },
    raw,
  )
  const expected = [data.common, data.en, data.source, data.changedBase, data.media, data.programs]
  expect(new Set(plan.blobs.map((blob) => blob.key))).toEqual(
    new Set(["common", "en", "source", "changed-base", "media", "programs"]),
  )
  const alias = { ...data.media, key: "media-alias", path: "alias.json" }
  data.files.set("media-alias", alias)
  const deduplicated = new FetchPlan({ ...plan.selection, files: data.files }, raw)
  expect(deduplicated.blobs).toHaveLength(expected.length)
  data.files.clear()
  const updates: FetchProgress[] = []
  await plan.run(new AbortController().signal, (progress) => updates.push(progress))
  const total = expected.reduce((bytes, file) => bytes + Number(file["bytes"]), 0)
  expect(updates.find((progress) => progress.phase === "downloading")).toMatchObject({
    total: expected.length,
    rawBytes: total,
    brotliBytes: total - expected.length,
    missing: expected.length - 1,
    missingRawBytes: total - Number(data.common["bytes"]),
  })
  expect(updates.at(-1)).toMatchObject({ phase: "complete", missing: 0, missingRawBytes: 0 })
  expect(data.network.mock.calls.map(([url]) => url)).not.toContain(`${base}/jp.json`)
  expect(data.network.mock.calls.map(([url]) => url)).not.toContain(`${base}/recursive.json`)
  expect(data.network.mock.calls.every(([, options]) => options?.speculative === true)).toBe(true)
  expect(data.network).toHaveBeenCalledTimes(expected.length - 1)
  const jpData = await fixture()
  const jp = new FetchPlan({ ...plan.selection, edition: "jp", files: jpData.files })
  expect(jp.blobs.map((blob) => blob.key)).toContain("jp")
  expect(jp.blobs.map((blob) => blob.key)).not.toContain("en")
})

it("counts persisted writes only and stops partially on failed storage or a cleared earlier file", async () => {
  for (const mode of [
    "blocked",
    "failed",
    "cleared",
    "unavailable",
    "quota-retry",
    "quota-stop",
  ] as const) {
    const data = await fixture()
    const files = new Map([
      ["common", data.common],
      ["programs", data.programs],
    ])
    const storage = memoryCache(mode === "failed")
    const name = `sve-raw:${encodeURIComponent(base)}`
    const cache = await storage.open(name)
    const open = storage.open.bind(storage)
    storage.open = (key) => (key === name ? Promise.resolve(cache) : open(key))
    const entered = Promise.withResolvers<undefined>()
    const gate = Promise.withResolvers<undefined>()
    const put = cache.put.bind(cache)
    const attempts = new Map<string, number>()
    if (mode.startsWith("quota")) {
      const evictable = await storage.open(`sve-snapshot:${encodeURIComponent(base)}`)
      await evictable.put(
        `${base}/unused`,
        new Response("x", {
          headers: { "x-sve-bytes": "1024", "x-sve-touched": "0" },
        }),
      )
      await evictable.put(
        `${base}/pointer/active`,
        new Response("active", {
          headers: { "x-sve-bytes": "6", "x-sve-touched": "0" },
        }),
      )
      // More work than the producer window makes stopping observable at the network boundary.
      for (const [key, file] of data.files) files.set(key, file)
    }
    cache.put = async (key, response) => {
      const url = typeof key === "string" ? key : key instanceof URL ? key.href : key.url
      attempts.set(url, (attempts.get(url) ?? 0) + 1)
      if (mode === "quota-stop" || (mode === "quota-retry" && attempts.get(url) === 1))
        throw new DOMException("quota", "QuotaExceededError")
      if (mode === "blocked") {
        entered.resolve(undefined)
        await gate.promise
      }
      await put(key, response)
      if (mode === "cleared") {
        for (const old of await cache.keys())
          if (old.url !== (typeof key === "string" ? key : key instanceof URL ? key.href : key.url))
            await cache.delete(old)
      }
    }
    const raw = new RawBlobStore(
      base,
      data.network,
      mode === "unavailable" ? undefined : storage,
      data.network,
    )
    if (mode.startsWith("quota")) await raw.retain(files, "adopted")
    const plan = new FetchPlan(
      { base, manifestHash: "adopted", dataVersion: "v1", edition: "jp", files },
      raw,
    )
    const updates: FetchProgress[] = []
    const running = plan.run(new AbortController().signal, (progress) => updates.push(progress))
    if (mode === "blocked") {
      await entered.promise
      expect(updates.at(-1)).toMatchObject({ phase: "downloading", missing: 2 })
      gate.resolve(undefined)
    }
    await running
    expect(updates.at(-1)?.phase).toBe(mode === "blocked" ? "complete" : "partial")
    if (mode !== "blocked") expect(updates.at(-1)?.missing).toBeGreaterThan(0)
    if (mode.startsWith("quota")) {
      expect(Math.max(...attempts.values())).toBeLessThanOrEqual(2)
      expect([...attempts.values()]).toContain(2)
      expect(data.network.mock.calls.length).toBeLessThan(files.size)
      expect(new Set(data.network.mock.calls.map(([url]) => url)).size).toBe(
        data.network.mock.calls.length,
      )
      expect(updates.at(-1)).toMatchObject({ phase: "partial", reason: "write" })
      expect(
        await (
          await storage.open(`sve-snapshot:${encodeURIComponent(base)}`)
        ).match(`${base}/pointer/active`),
      ).toBeDefined()
    }
    if (mode === "unavailable") expect(data.network).not.toHaveBeenCalled()
  }
})

it("keeps whole-inventory enumeration bounded as a raw batch grows", async () => {
  const scans: { files: number; inventories: number; rawKeys: number }[] = []
  for (const count of [4, 40]) {
    const storage = memoryCache()
    const cache = await storage.open(`sve-raw:${encodeURIComponent(base)}`)
    const open = storage.open.bind(storage)
    storage.open = (name) => (name.startsWith("sve-raw:") ? Promise.resolve(cache) : open(name))
    const inventories = vi.spyOn(storage, "keys")
    const keys = vi.spyOn(cache, "keys")
    const files = new Map<string, JsonObject>()
    const bodies = new Map<string, Uint8Array>()
    for (let i = 0; i < count; i += 1) {
      const key = `file-${String(i)}`
      const body = new TextEncoder().encode(key)
      bodies.set(`${base}/${key}.json`, body)
      files.set(key, {
        key,
        path: `${key}.json`,
        sha256: await transferDigest(body),
        bytes: body.length,
        role: "text",
        region: "jp",
        compressed_bytes: { br: 1 },
        dependencies: [],
        source_refs: [],
      })
    }
    const network = vi.fn<Fetcher>((url) =>
      Promise.resolve(new Response(bodies.get(url)?.slice().buffer)),
    )
    const raw = new RawBlobStore(base, network, storage, network)
    const plan = new FetchPlan(
      { base, manifestHash: "batch", dataVersion: "v1", edition: "jp", files },
      raw,
    )
    const updates: FetchProgress[] = []
    await plan.run(new AbortController().signal, (progress) => updates.push(progress))
    expect(updates.at(-1)).toMatchObject({ phase: "complete", total: count, missing: 0 })
    expect(network).toHaveBeenCalledTimes(count)
    scans.push({
      files: count,
      inventories: inventories.mock.calls.length,
      rawKeys: keys.mock.calls.length,
    })
  }
  expect(scans[1]?.inventories).toBe(scans[0]?.inventories)
  expect(scans[1]?.rawKeys).toBe(scans[0]?.rawKeys)
  expect(scans[1]?.inventories).toBeLessThanOrEqual(2)
  expect(scans[1]?.rawKeys).toBeLessThanOrEqual(2)
})
