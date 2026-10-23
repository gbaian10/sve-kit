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
  for (const mode of ["blocked", "failed", "cleared", "unavailable"] as const) {
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
    cache.put = async (key, response) => {
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
    if (mode === "unavailable") expect(data.network).not.toHaveBeenCalled()
  }
})
