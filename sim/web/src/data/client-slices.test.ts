// @vitest-environment node
import { MessageChannel as NodeChannel } from "node:worker_threads"

import { afterEach, describe, expect, it, vi } from "vitest"

import { buildSnapshot } from "../../scripts/fixture/build"
import { memoryCache } from "../test-utils/cache"
import { catalogOf, prepareCatalog } from "./catalog"
import type { Fetcher } from "./cdn"
import { createSnapshotClient, type LoadedSnapshot, type SnapshotClient } from "./client"
import {
  arrayValue,
  canonical,
  type JsonObject,
  objectValue,
  parseJson,
  stringValue,
} from "./format-v3/json"
import { transferDigest } from "./integrity"
import { VerifiedCache } from "./verified-cache"

const base = "https://slices.test"
const built = await buildSnapshot({ encodeImage: () => Promise.resolve(new Uint8Array([1])) })
const INDEX = "snapshots/versions/index.json"
const fixtureIndex = objectValue(parseJson(built.files.get(INDEX) ?? new Uint8Array()))
const manifest = { ...built.manifest, data_version: "20261018T000000Z-0001" }
const bytes = canonical(manifest),
  hash = await transferDigest(bytes),
  manifestPath = `snapshots/manifests/${hash.slice(7)}.json`
const nextIndex = canonical({
  ...fixtureIndex,
  revision: Number(fixtureIndex["revision"]) + 1,
  current: {
    ...objectValue(fixtureIndex["current"]),
    data_version: manifest["data_version"],
    manifest_path: manifestPath,
    manifest_sha256: hash,
  },
})

function origin() {
  const files = new Map(built.files)
  files.set(manifestPath, bytes)
  const paths: string[] = []
  let current = files.get(INDEX)
  const fetch: Fetcher = (url) => {
    const path = url.slice(base.length + 1)
    paths.push(path)
    const data = path === INDEX ? current : files.get(path)
    return Promise.resolve(
      data ? new Response(data.slice().buffer) : new Response(null, { status: 404 }),
    )
  }
  return {
    fetch,
    paths,
    update: () => {
      current = nextIndex
    },
    badMetadata: async () => {
      const damaged: JsonObject = structuredClone(manifest)
      const file = arrayValue(damaged["files"])
        .map((value) => objectValue(value))
        .find((file) => file["role"] === "images")
      if (!file) throw new Error("Missing image descriptor")
      file["row_counts"] = null
      const bytes = canonical(damaged),
        sha = await transferDigest(bytes),
        path = `snapshots/manifests/${sha.slice(7)}.json`
      files.set(path, bytes)
      current = canonical({
        ...fixtureIndex,
        revision: Number(fixtureIndex["revision"]) + 1,
        current: {
          ...objectValue(fixtureIndex["current"]),
          data_version: damaged["data_version"] ?? null,
          manifest_path: path,
          manifest_sha256: sha,
        },
      })
    },
    fallback: () => {
      current = canonical({
        ...fixtureIndex,
        revision: Number(fixtureIndex["revision"]) + 1,
        current: {
          ...objectValue(fixtureIndex["current"]),
          format_version: "99.0.0",
          data_version: manifest["data_version"],
        },
        previous: fixtureIndex["current"] ?? null,
      })
    },
  }
}
function observed(client: SnapshotClient) {
  const snapshots: LoadedSnapshot[] = []
  let previous = client.snapshot()
  client.subscribe(() => {
    const snapshot = client.snapshot()
    if (snapshot && snapshot !== previous) {
      snapshots.push(snapshot)
      previous = snapshot
    }
  })
  return snapshots
}
function ready(client: SnapshotClient) {
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("No adopted catalog")
  expect(client.status().state).toBe("ready")
  expect(catalogOf(snapshot).index.card("c:bp01-001")?.["home_set_id"]).toBe("set:bp01")
  return snapshot
}
// The platform clock forces budget cuts without depending on the host's speed.
function slices() {
  let clock = performance.now()
  vi.spyOn(performance, "now").mockImplementation(() => (clock += 9))
  vi.stubGlobal("requestIdleCallback", () => 0)
  const measures: { name: string; duration: number }[] = []
  let stage = ""
  const measure = performance.measure.bind(performance)
  vi.spyOn(performance, "measure").mockImplementation((...args) => {
    stage = args[0]
    const entry = measure(...args)
    measures.push({ name: entry.name, duration: entry.duration })
    return entry
  })
  let held:
    | {
        entered: PromiseWithResolvers<undefined>
        released: PromiseWithResolvers<undefined>
        stage?: string
      }
    | undefined
  vi.stubGlobal("scheduler", {
    yield: () => {
      const gate = held
      if (!gate || (gate.stage && gate.stage !== stage)) return Promise.resolve(undefined)
      held = undefined
      gate.entered.resolve(undefined)
      return gate.released.promise
    },
  })
  return {
    measures,
    advance: (ms: number) => {
      clock += ms
    },
    hold: (stage?: string) => {
      const gate = {
        entered: Promise.withResolvers<undefined>(),
        released: Promise.withResolvers<undefined>(),
        ...(stage ? { stage } : {}),
      }
      held = gate
      return {
        entered: gate.entered.promise,
        release: () => {
          gate.released.resolve(undefined)
        },
      }
    },
  }
}
afterEach(() => {
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
})

const persisted = async (storage: CacheStorage) => {
  const cache = await storage.open(`sve-snapshot:${encodeURIComponent(base)}`)
  await expect
    .poll(async () => (await cache.keys()).some((key) => key.url.includes("/adopted-index-jp/")))
    .toBe(true)
  return cache
}
async function seeded() {
  const storage = memoryCache(),
    served = origin()
  const original = createSnapshotClient(base, { fetch: served.fetch, cacheStorage: storage })
  await original.load()
  await persisted(storage)
  return { storage, original }
}

describe("catalog preparation at client publication boundaries", () => {
  it("keeps a download private until complete and publishes one prepared snapshot", async () => {
    const clock = slices(),
      gate = clock.hold(),
      served = origin()
    const client = createSnapshotClient(base, { fetch: served.fetch, cacheStorage: memoryCache() })
    const published = observed(client),
      loading = client.load()
    await gate.entered
    expect(client.snapshot()).toBeNull()
    expect(published).toEqual([])
    gate.release()
    await loading
    const snapshot = ready(client),
      catalog = catalogOf(snapshot)
    expect(published).toEqual([snapshot])
    await client.reload()
    expect(client.snapshot()).toBe(snapshot)
    expect(catalogOf(snapshot)).toBe(catalog)
    expect(published).toEqual([snapshot])
  })

  it("retains the previous catalog through a sliced update and adopts the result only once", async () => {
    const clock = slices(),
      served = origin(),
      client = createSnapshotClient(base, { fetch: served.fetch, cacheStorage: memoryCache() })
    await client.load()
    const before = ready(client),
      published = observed(client)
    served.update()
    const gate = clock.hold(),
      reloading = client.reload()
    await gate.entered
    expect(client.snapshot()).toBe(before)
    expect(published).toEqual([])
    gate.release()
    await reloading
    const after = ready(client)
    expect(after.dataVersion).toBe(manifest["data_version"])
    expect(published.filter((value) => value !== before)).toEqual([after])
  })

  it.each(["download", "update"])(
    "discards an obsolete %s generation before adoption",
    async (path) => {
      const clock = slices(),
        served = origin(),
        client = createSnapshotClient(base, { fetch: served.fetch, cacheStorage: memoryCache() })
      if (path === "update") {
        await client.load()
        served.update()
      }
      const before = client.snapshot(),
        published = observed(client),
        gate = clock.hold()
      const loading = path === "download" ? client.load() : client.reload()
      await gate.entered
      const selecting = client.selectRegion?.("en")
      gate.release()
      await Promise.all([loading, selecting])
      const after = ready(client)
      expect(after.region).toBe("en")
      expect(published.filter((value) => value !== before)).toEqual([after])
      expect(client.status()).not.toHaveProperty("updateError")
    },
  )

  it.each(["restore", "restoreAdopted"])(
    "prepares a %s record before its single publication",
    async (path) => {
      const { storage } = await seeded()
      if (path === "restore") {
        const cache = await persisted(storage)
        for (const key of await cache.keys())
          if (key.url.includes("/adopted-")) await cache.delete(key)
      }
      const clock = slices(),
        gate = clock.hold("sve:catalog:catalogOf"),
        served = origin(),
        client = createSnapshotClient(base, { fetch: served.fetch, cacheStorage: storage })
      const published = observed(client),
        loading = client.load()
      await gate.entered
      expect(client.snapshot()).toBeNull()
      expect(published).toEqual([])
      gate.release()
      await loading
      await client.reload()
      const snapshot = ready(client)
      expect(published).toEqual([snapshot])
      expect(served.paths.every((path) => path === INDEX)).toBe(true)
      expect(clock.measures.some((entry) => entry.name === "sve:catalog:catalogOf")).toBe(true)
      expect(clock.measures.some((entry) => entry.name === "sve:catalog:validate")).toBe(false)
    },
  )

  it.each(["restore", "restoreAdopted"])(
    "cancels a stale %s without removing the verified record",
    async (path) => {
      const { storage, original } = await seeded(),
        hash = ready(original).manifestHash
      if (path === "restore") {
        const cache = await persisted(storage)
        for (const key of await cache.keys())
          if (key.url.includes("/adopted-")) await cache.delete(key)
      }
      const clock = slices(),
        gate = clock.hold(),
        served = origin(),
        client = createSnapshotClient(base, { fetch: served.fetch, cacheStorage: storage })
      const published = observed(client),
        loading = client.load()
      await gate.entered
      const selecting = client.selectRegion?.("en")
      gate.release()
      await Promise.all([loading, selecting])
      const snapshot = ready(client)
      expect(snapshot.region).toBe("en")
      expect(published).toEqual([snapshot])
      expect(
        await new VerifiedCache(base, "snapshot", 2, storage).read("snapshot-jp", hash),
      ).toBeDefined()
    },
  )

  it.each([false, true])(
    "cancels comparison preparation with loaded=%s and permits a clean retry",
    async (loaded) => {
      const clock = slices(),
        served = origin(),
        client = createSnapshotClient(base, { fetch: served.fetch, cacheStorage: memoryCache() })
      if (loaded) await client.load()
      const before = client.snapshot(),
        published = observed(client),
        gate = clock.hold(),
        abort = new AbortController()
      const comparing = client.catalog?.("en", abort.signal)
      const rejected = expect(comparing).rejects.toMatchObject({ name: "AbortError" })
      await gate.entered
      abort.abort()
      gate.release()
      await rejected
      expect(client.snapshot()).toBe(before)
      const compared = await client.catalog?.("en", new AbortController().signal)
      expect(compared?.region).toBe("en")
      expect(published.filter((value) => value !== before)).toEqual([])
      if (loaded && compared) {
        const cached = catalogOf(compared)
        await client.selectRegion?.("en")
        expect(client.snapshot()).toBe(compared)
        expect(catalogOf(ready(client))).toBe(cached)
        expect(published.filter((value) => value !== before)).toEqual([compared])
      }
    },
  )

  it("rejects a stale comparison generation while selecting another region", async () => {
    const clock = slices(),
      served = origin(),
      client = createSnapshotClient(base, { fetch: served.fetch, cacheStorage: memoryCache() })
    await client.load()
    const gate = clock.hold(),
      published = observed(client)
    const comparing = client.catalog?.("en", new AbortController().signal)
    const rejected = expect(comparing).rejects.toMatchObject({ name: "AbortError" })
    await gate.entered
    const selecting = client.selectRegion?.("en")
    gate.release()
    await Promise.all([rejected, selecting])
    expect(published.map((snapshot) => snapshot.region)).toEqual(["en"])
    ready(client)
  })

  it("prepares complete printings privately and notifies once after supplementation", async () => {
    const clock = slices(),
      served = origin(),
      client = createSnapshotClient(base, { fetch: served.fetch, cacheStorage: memoryCache() })
    await client.load()
    clock.measures.length = 0
    const before = ready(client),
      published = observed(client),
      gate = clock.hold("sve:catalog:catalogOf")
    const printing = client.prefetchPrintings?.()
    await gate.entered
    expect(client.snapshot()).toBe(before)
    expect(before.printingReady).not.toBe(true)
    gate.release()
    await printing
    const after = ready(client)
    expect(after.printingReady).toBe(true)
    expect(published).toEqual([after])
    expect(clock.measures.some((entry) => entry.name === "sve:catalog:catalogOf")).toBe(true)
    expect(clock.measures.some((entry) => entry.name === "sve:catalog:validate")).toBe(false)
    expect(catalogOf(after).summary("p:bp01-002-alt")?.cardNo).toBe("BP01-002a")
    await client.prefetchPrintings?.()
    expect(published).toEqual([after])
  })

  it("does not publish obsolete printing preparation after a region change", async () => {
    const clock = slices(),
      served = origin(),
      client = createSnapshotClient(base, { fetch: served.fetch, cacheStorage: memoryCache() })
    await client.load()
    const before = ready(client),
      published = observed(client),
      gate = clock.hold()
    const printing = client.prefetchPrintings?.()
    await gate.entered
    const selecting = client.selectRegion?.("en")
    gate.release()
    await Promise.all([printing, selecting])
    const after = ready(client)
    expect(after.region).toBe("en")
    expect(published.filter((value) => value !== before)).toEqual([after])
    expect(after.printingReady).not.toBe(true)
  })

  it("ignores a failed download from an obsolete selection", async () => {
    const served = origin(),
      held = Promise.withResolvers<undefined>(),
      entered = Promise.withResolvers<undefined>()
    const target = (built.manifest["files"] as import("./format-v3/json").JsonObject[]).find(
      (file) => file["purpose"] === "catalog" && file["region"] === "jp",
    )
    if (!target) throw new Error("Missing JP file")
    let blocked = false
    const client = createSnapshotClient(base, {
      cacheStorage: memoryCache(),
      fetch: async (url, init) => {
        if (!blocked && url.endsWith(stringValue(target["path"]))) {
          blocked = true
          entered.resolve(undefined)
          await held.promise
          throw new TypeError("obsolete network failure")
        }
        return served.fetch(url, init)
      },
    })
    const published = observed(client),
      loading = client.load()
    await entered.promise
    const selecting = client.selectRegion?.("en")
    held.resolve(undefined)
    await Promise.all([loading, selecting])
    expect(ready(client).region).toBe("en")
    expect(published.map((snapshot) => snapshot.region)).toEqual(["en"])
  })

  it.each(["reload", "restored background check"])(
    "keeps completed printings when a same-manifest %s finishes later",
    async (path) => {
      const saved = path === "reload" ? undefined : await seeded()
      slices()
      const served = origin(),
        entered = Promise.withResolvers<undefined>(),
        released = Promise.withResolvers<undefined>()
      let holdIndex = path !== "reload"
      const client = createSnapshotClient(base, {
        cacheStorage: saved?.storage ?? memoryCache(),
        fetch: async (url, init) => {
          if (holdIndex && url === `${base}/${INDEX}`) {
            holdIndex = false
            entered.resolve(undefined)
            await released.promise
          }
          return served.fetch(url, init)
        },
      })
      await client.load()
      if (path === "reload") holdIndex = true
      const reloading = client.reload()
      await entered.promise
      await client.prefetchPrintings?.()
      const supplemented = ready(client),
        catalog = catalogOf(supplemented),
        published = observed(client)
      expect(supplemented.printingReady).toBe(true)
      released.resolve(undefined)
      await reloading
      expect(client.snapshot()).toBe(supplemented)
      expect(catalogOf(ready(client))).toBe(catalog)
      expect(published).toEqual([])
      await client.prefetchPrintings?.()
      expect(client.snapshot()).toBe(supplemented)
      expect(published).toEqual([])
    },
  )

  it("keeps a restored snapshot record after a transient preparation failure", async () => {
    const { storage, original } = await seeded(),
      snapshot = ready(original),
      stored = new VerifiedCache(base, "snapshot", 2, storage),
      before = await stored.read("snapshot-jp", snapshot.manifestHash)
    expect(before).toBeDefined()
    slices()
    let fail = true
    vi.stubGlobal("scheduler", {
      yield: () => {
        if (!fail) return Promise.resolve()
        fail = false
        return Promise.reject(new Error("temporary scheduler failure"))
      },
    })
    const served = origin(),
      released = Promise.withResolvers<undefined>(),
      client = createSnapshotClient(base, {
        cacheStorage: storage,
        fetch: async (url, init) => {
          if (url === `${base}/${INDEX}`) await released.promise
          return served.fetch(url, init)
        },
      }),
      loading = client.load()
    try {
      await expect.poll(() => stored.read("adopted-index-jp", "active")).toBeUndefined()
      expect(await stored.read("snapshot-jp", snapshot.manifestHash)).toEqual(before)
    } finally {
      released.resolve(undefined)
      await loading
    }
    expect(ready(client).manifestHash).toBe(snapshot.manifestHash)
    expect(served.paths.every((path) => path === INDEX)).toBe(true)
  })

  it("merges printing completion that wins a yield before a same-hash adoption", async () => {
    const clock = slices(),
      served = origin(),
      client = createSnapshotClient(base, { fetch: served.fetch, cacheStorage: memoryCache() })
    await client.load()
    served.fallback()
    const gate = clock.hold(),
      reloading = client.reload()
    await gate.entered
    await client.prefetchPrintings?.()
    const completed = ready(client),
      published = observed(client)
    expect(completed.printingReady).toBe(true)
    gate.release()
    await reloading
    const adopted = ready(client)
    expect(adopted.entrySource).toBe("previous")
    expect(catalogOf(adopted).summary("p:bp01-002-alt")?.rarityCode).toBe("silver")
    expect(published).toEqual([adopted])
  })

  it.each(["download", "restore", "restoreAdopted", "comparison", "printing"])(
    "rejects obsolete %s preparation during index construction",
    async (path) => {
      const saved = path === "restore" || path === "restoreAdopted" ? await seeded() : undefined
      if (path === "restore" && saved) {
        const cache = await persisted(saved.storage)
        for (const key of await cache.keys())
          if (key.url.includes("/adopted-")) await cache.delete(key)
      }
      const clock = slices(),
        served = origin(),
        client = createSnapshotClient(base, {
          fetch: served.fetch,
          cacheStorage: saved?.storage ?? memoryCache(),
        })
      if (path === "comparison" || path === "printing") await client.load()
      const before = client.snapshot(),
        published = observed(client),
        gate = clock.hold("sve:catalog:catalogOf")
      const abort = new AbortController()
      const running =
        path === "comparison"
          ? client.catalog?.("en", abort.signal)
          : path === "printing"
            ? client.prefetchPrintings?.()
            : client.load()
      const finished =
        path === "comparison"
          ? expect(running).rejects.toMatchObject({ name: "AbortError" })
          : running
      await gate.entered
      expect(client.snapshot()).toBe(before)
      const selecting =
        path === "comparison" ? (abort.abort(), undefined) : client.selectRegion?.("en")
      gate.release()
      await Promise.all([finished, selecting])
      if (path === "comparison") {
        expect(client.snapshot()).toBe(before)
        expect(published).toEqual([])
      } else {
        const after = ready(client)
        expect(after.region).toBe("en")
        expect(published).toEqual([after])
      }
    },
  )

  it.each(["restore", "restoreAdopted"])(
    "rebuilds an unusable %s record when catalog construction fails",
    async (path) => {
      const { storage, original } = await seeded(),
        snapshot = ready(original)
      const stored = new VerifiedCache(base, "snapshot", 2, storage)
      const bootstrap = structuredClone(snapshot.bootstrap)
      const card = bootstrap.find((fragment) => fragment.table === "card")?.rows[0]
      if (!card) throw new Error("Missing fixture card")
      delete card["id"]
      const record = {
        manifest: snapshot.manifest,
        config: snapshot.config,
        bootstrap,
      }
      await stored.write(
        "snapshot-jp",
        snapshot.manifestHash,
        new TextEncoder().encode(JSON.stringify(record)),
      )
      if (path === "restore") {
        const cache = await persisted(storage)
        for (const key of await cache.keys())
          if (key.url.includes("/adopted-")) await cache.delete(key)
      }
      const served = origin(),
        client = createSnapshotClient(base, { fetch: served.fetch, cacheStorage: storage }),
        published = observed(client)
      await client.load()
      await client.reload()
      expect(published).toEqual([ready(client)])
      expect(served.paths.some((path) => path.startsWith("snapshots/manifests/"))).toBe(true)
    },
  )

  it("excludes task-yield waits from active work observations", async () => {
    const clock = slices(),
      served = origin(),
      client = createSnapshotClient(base, { fetch: served.fetch, cacheStorage: memoryCache() })
    await client.load()
    const candidate = { ...ready(client) },
      first = clock.measures.length,
      gate = clock.hold()
    const preparing = prepareCatalog(candidate, () => undefined)
    await gate.entered
    clock.advance(10_000)
    gate.release()
    await preparing
    const work = clock.measures
      .slice(first)
      .filter((entry) => ["sve:catalog:validate", "sve:catalog:catalogOf"].includes(entry.name))
    expect(new Set(work.map((entry) => entry.name))).toEqual(
      new Set(["sve:catalog:validate", "sve:catalog:catalogOf"]),
    )
    expect(work.every((entry) => entry.duration < 100)).toBe(true)
    expect(performance.getEntriesByName("sve:catalog:validate")).toEqual([])
  })

  it("closes both fallback ports after cancellation during a yielded comparison", async () => {
    const served = origin(),
      client = createSnapshotClient(base, { fetch: served.fetch, cacheStorage: memoryCache() })
    await client.load()
    const before = ready(client),
      abort = new AbortController(),
      closed: Promise<void>[] = [],
      channels: NodeChannel[] = []
    let clock = performance.now()
    vi.spyOn(performance, "now").mockImplementation(() => (clock += 1))
    vi.stubGlobal("scheduler", undefined)
    vi.stubGlobal(
      "MessageChannel",
      class extends NodeChannel {
        constructor() {
          super()
          channels.push(this)
          closed.push(
            new Promise((resolve) =>
              this.port1.once("close", () => {
                resolve()
              }),
            ),
            new Promise((resolve) =>
              this.port2.once("close", () => {
                resolve()
              }),
            ),
          )
          const post = this.port2.postMessage.bind(this.port2)
          this.port2.postMessage = (value: unknown) => {
            abort.abort()
            post(value)
          }
        }
      },
    )
    const comparing = client.catalog?.("en", abort.signal)
    await expect(comparing).rejects.toMatchObject({ name: "AbortError" })
    expect(channels.length).toBeGreaterThan(0)
    await Promise.all(closed)
    expect(client.snapshot()).toBe(before)
  })

  it("starts a new printing job when the same manifest has a different adopted snapshot", async () => {
    const clock = slices(),
      served = origin(),
      client = createSnapshotClient(base, { fetch: served.fetch, cacheStorage: memoryCache() })
    await client.load()
    const gate = clock.hold(),
      obsolete = client.prefetchPrintings?.()
    await gate.entered
    served.fallback()
    await client.reload()
    const replacement = ready(client),
      published = observed(client)
    expect(replacement.printingReady).not.toBe(true)
    await client.prefetchPrintings?.()
    const completed = ready(client)
    expect(completed.printingReady).toBe(true)
    gate.release()
    await obsolete
    expect(client.snapshot()).toBe(completed)
    expect(published).toEqual([completed])
  })

  it("coalesces a selection requested synchronously by a reload progress listener", async () => {
    const served = origin(),
      client = createSnapshotClient(base, { fetch: served.fetch, cacheStorage: memoryCache() })
    await client.load()
    let selected = false,
      selecting: Promise<void> | undefined
    const published = observed(client)
    client.subscribe(() => {
      const status = client.status()
      if (!selected && status.state === "ready" && status.updating === "index") {
        selected = true
        selecting = client.selectRegion?.("en")
      }
    })
    await client.reload()
    await selecting
    expect(ready(client).region).toBe("en")
    expect(published.map((snapshot) => snapshot.region)).toEqual(["en"])
  })

  it("keeps the previous snapshot when adoption cannot prepare its metadata", async () => {
    const served = origin(),
      client = createSnapshotClient(base, { fetch: served.fetch, cacheStorage: memoryCache() })
    await client.load()
    const before = ready(client),
      published = observed(client)
    await served.badMetadata()
    await client.reload()
    expect(client.snapshot()).toBe(before)
    expect(published).toEqual([])
    expect(client.status()).toMatchObject({ state: "ready", updateError: { kind: "corrupt" } })
  })

  it("uses the MessageChannel fallback without publishing a partial catalog", async () => {
    let clock = performance.now()
    vi.spyOn(performance, "now").mockImplementation(() => (clock += 0.1))
    vi.stubGlobal("scheduler", undefined)
    vi.stubGlobal("requestIdleCallback", () => 0)
    const served = origin(),
      client = createSnapshotClient(base, { fetch: served.fetch, cacheStorage: memoryCache() }),
      published = observed(client)
    await client.load()
    expect(published).toEqual([ready(client)])
  })
})
