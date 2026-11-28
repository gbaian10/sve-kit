// @vitest-environment node
import { describe, expect, it } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { CREDITED_ARTIST, UNCREDITED_ARTIST } from "../../domain/artistFilter"
import { cardNoKey } from "../../domain/cardNo"
import { LOCATION_ORDER } from "../../domain/deckLocation"
import { parseQuery } from "../../domain/query/codec"
import { DEFAULT_QUERY } from "../../domain/query/model"
import { apply } from "../../domain/search"
import { memoryCache } from "../../test-utils/cache"
import { sharedAnnotatedTextResolver } from "../annotated-text"
import { catalogOf } from "../catalog"
import type { Fetcher } from "../cdn"
import { createSnapshotClient } from "../client"
import {
  arrayValue,
  canonical,
  integerValue,
  type JsonObject,
  objectValue,
  stringValue,
} from "../format-v3/json"
import { columns, rowType } from "../format-v3/schema"
import { digest } from "../format-v3/sha256"
import {
  type SearchChannel,
  type SearchChannelStatus,
  type SearchQuery,
  SearchSession,
  type SearchStatus,
} from "../index"
const numeric = new Intl.Collator("en", { numeric: true, sensitivity: "base" })
function orderExpected(
  catalog: ReturnType<typeof catalogOf>,
  items: { key: string; printingId: string }[],
) {
  return items.sort((a, b) => {
    const first = cardNoKey(catalog.summary(a.printingId)?.cardNo ?? "", catalog.sets)
    const second = cardNoKey(catalog.summary(b.printingId)?.cardNo ?? "", catalog.sets)
    if (!first || !second) return Number(second !== null) - Number(first !== null)
    return (
      numeric.compare(first.set, second.set) ||
      numeric.compare(first.number, second.number) ||
      numeric.compare(first.suffix.replace(/en$/u, ""), second.suffix.replace(/en$/u, "")) ||
      a.key.localeCompare(b.key) ||
      a.printingId.localeCompare(b.printingId)
    )
  })
}
type CombinedStatus = SearchStatus & SearchChannelStatus
import { BootstrapColumns } from "./bootstrap"
import { SearchEngine } from "./engine"
import { SearchIndex } from "./index"
import type { BootstrapPlan, SearchEvent, SearchRequest } from "./messages"
import { uniquePromoLeader } from "./promo-leader"
import type { SearchTransportFactory } from "./session"

const built = await buildSnapshot({
  encodeImage: ({ seed }) => Promise.resolve(new TextEncoder().encode(String(seed))),
})
// Legacy catalog comparisons exercise explicit card grouping alongside the printing default.
const { order: _defaultOrder, ...ascendingQuery } = DEFAULT_QUERY
const CARD_QUERY = { ...ascendingQuery, sort: "no" as const, unit: "card" as const }
const query: SearchQuery = { state: CARD_QUERY, edition: "jp", limit: 100 }

it("chooses the uniquely largest promo family and hides ties", () => {
  const promos = [
    {
      id: "pr",
      code: "PR",
      name: {
        original: { lang: "ja" as const, text: "" },
        translations: { "zh-Hant": "", en: "" },
      },
    },
    {
      id: "bsf24",
      code: "BSF2024",
      name: {
        original: { lang: "ja" as const, text: "" },
        translations: { "zh-Hant": "", en: "" },
      },
    },
    {
      id: "bsf25",
      code: "BSF2025",
      name: {
        original: { lang: "ja" as const, text: "" },
        translations: { "zh-Hant": "", en: "" },
      },
    },
  ]
  expect(uniquePromoLeader(promos, { PR: 557, BSF2024: 1, BSF2025: 1 })?.code).toBe("PR")
  expect(uniquePromoLeader(promos, { PR: 2, BSF2024: 2, BSF2025: 1 })).toBeUndefined()
})

const base = "https://cdn.test"
function origin() {
  const files = new Map(built.files)
  const requests: string[] = []
  let held: { path: string; promise: Promise<void> } | undefined
  let pointer = canonical({
    manifest_path: built.manifestPath,
    manifest_sha256: digest(files.get(built.manifestPath) ?? new Uint8Array()),
  })
  const fetcher: Fetcher = async (url) => {
    const path = url.slice(base.length + 1)
    requests.push(path)
    if (held?.path === path) await held.promise
    const data = path === "snapshots/preview/current.json" ? pointer : files.get(path)
    return data ? new Response(data.slice().buffer) : new Response(null, { status: 404 })
  }
  return {
    files,
    requests,
    fetcher,
    hold(path: string) {
      const gate = Promise.withResolvers<undefined>()
      held = { path, promise: gate.promise }
      return () => {
        gate.resolve(undefined)
      }
    },
    replaceManifest(manifest: JsonObject) {
      const bytes = canonical(manifest)
      const path = `snapshots/manifests/${digest(bytes).slice(7)}.json`
      files.set(path, bytes)
      pointer = canonical({ manifest_path: path, manifest_sha256: digest(bytes) })
    },
    update(number: number) {
      const manifest = objectValue(JSON.parse(JSON.stringify(built.manifest)) as JsonObject)
      manifest["data_version"] = `20261004T01000${String(number)}Z-0001`
      const data = canonical(manifest)
      const path = `snapshots/manifests/${digest(data).slice(7)}.json`
      files.set(path, data)
      pointer = canonical({ manifest_path: path, manifest_sha256: digest(data) })
    },
  }
}
function harness(served = origin(), cacheStorage = memoryCache(), buildId = "build") {
  const events: SearchEvent[] = []
  const requests: SearchRequest[] = []
  let pending = Promise.resolve()
  let crash: () => void = () => undefined
  const factory: SearchTransportFactory = (receive, failed) => {
    crash = failed
    const engine = new SearchEngine(
      (event) => {
        events.push(event)
        receive(event)
      },
      { fetch: served.fetcher, cacheStorage, buildId },
    )
    return {
      send(request) {
        requests.push(request)
        const task = engine.handle(request)
        if (request.kind === "load") pending = task
      },
      dispose() {
        void engine.handle({
          kind: "cancel",
          generation: requests.findLast((request) => request.kind === "load")?.generation ?? 0,
        })
      },
    }
  }
  const session = new SearchSession(factory)
  const channel: SearchChannel = session.createChannel()
  return {
    channel,
    status: (): CombinedStatus => ({ ...session.getStatus(), ...channel.getStatus() }),
    session,
    events,
    requests,
    served,
    crash: () => {
      crash()
    },
    queried: () => new Promise((resolve) => setTimeout(resolve, 0)),
    settled: async () => {
      await pending
      await new Promise((resolve) => setTimeout(resolve, 0))
    },
  }
}
/** Real message passing is asynchronous: a ready can cross a cancel or a newer load in flight. */
function crossing(served = origin()) {
  const toWorker: SearchRequest[] = []
  const toMain: SearchEvent[] = []
  let receive: (event: SearchEvent) => void = () => undefined
  const engine = new SearchEngine((event) => toMain.push(event), {
    fetch: served.fetcher,
    cacheStorage: memoryCache(),
  })
  const session = new SearchSession((callback) => {
    receive = callback
    return {
      send: (request) => toWorker.push(request),
      dispose: () => undefined,
    }
  })
  const channel: SearchChannel = session.createChannel()
  return {
    channel,
    status: (): CombinedStatus => ({ ...session.getStatus(), ...channel.getStatus() }),
    session,
    served,
    worker: () => Promise.all(toWorker.splice(0).map((request) => engine.handle(request))),
    main: () => {
      for (const event of toMain.splice(0)) receive(event)
    },
  }
}
const bootstrapFile = (built.manifest["files"] as JsonObject[]).find(
  (file) => file["purpose"] === "catalog" && file["region"] === "jp",
)
if (!bootstrapFile) throw new Error("fixture has no bootstrap")
const heldPath = stringValue(bootstrapFile["path"])
// The held file is reached only after the manifest is hashed and decoded, which a busy machine
// can stretch past the default one-second poll.
const POLL = { timeout: 10_000 }
// A card's representative may be a printing from another set, which is a locator until the
// background printing download lands; `search` channels do not re-run on their own.
const completions = (events: readonly SearchEvent[]) =>
  events.filter((event) => event.kind === "background-ready" && event.purpose === "mechanics")
    .length

describe("complete Worker catalog generations", () => {
  it("cancel/restart never publishes partial data, and retry rebuilds from cache", async () => {
    const h = harness()
    const release = h.served.hold(heldPath)
    h.session.load(base, "jp", "preview")
    h.channel.search(query)
    h.session.cancel()
    release()
    await h.settled()
    expect(h.events.some((event) => event.kind === "ready")).toBe(false)
    h.session.retry()
    await h.settled()
    expect(h.status().result.phase).toBe("complete")
    const count = h.served.requests.filter((path) => path.startsWith("snapshots/blobs/")).length
    h.crash()
    expect(h.status()).toMatchObject({ phase: "error", generation: null })
    h.session.retry()
    await h.settled()
    expect(h.status().result.phase).toBe("complete")
    expect(h.served.requests.filter((path) => path.startsWith("snapshots/blobs/"))).toHaveLength(
      count,
    )
  })
})

it("a ready that crossed a cancel stays the generation queries go to", async () => {
  const h = crossing()
  h.session.load(base, "jp", "preview")
  await h.worker()
  h.main()
  h.session.load(base, "jp", "preview")
  await h.worker()
  h.session.cancel()
  await h.worker()
  h.main()
  h.channel.search(query)
  await h.worker()
  h.main()
  expect(h.status()).toMatchObject({
    phase: "ready",
    generation: 2,
    updating: false,
    result: { phase: "complete", generation: 2 },
  })
})

it("a replaced load that finished first serves queries after its replacement fails", async () => {
  const h = crossing()
  h.session.load(base, "jp", "preview")
  await h.worker()
  h.main()
  h.session.load(base, "jp", "preview")
  await h.worker()
  h.served.update(3)
  h.served.files.clear()
  h.session.load(base, "jp", "preview")
  await h.worker()
  h.main()
  h.channel.search(query)
  await h.worker()
  h.main()
  expect(h.status()).toMatchObject({
    phase: "ready",
    generation: 2,
    updating: false,
    result: { phase: "complete", generation: 2 },
  })
  expect(h.status().error?.message).toContain("HTTP 404")
})

it("a load after the Worker failed starts a new Worker and drops the old error", () => {
  const workers: SearchRequest[][] = []
  const failures: (() => void)[] = []
  const session = new SearchSession((_receive, failed) => {
    const sent: SearchRequest[] = []
    workers.push(sent)
    failures.push(failed)
    return {
      send: (request) => sent.push(request),
      dispose: () => undefined,
    }
  })
  const channel: SearchChannel = session.createChannel()
  session.load(base, "jp")
  channel.search(query)
  failures[0]?.()
  expect(channel.getStatus().result.phase).toBe("error")
  session.load(base, "en")
  expect(workers).toHaveLength(2)
  expect(workers[1]).toMatchObject([{ kind: "load", edition: "en" }])
  expect(session.getStatus()).toMatchObject({ phase: "downloading" })
  expect(channel.getStatus().result.phase).toBe("pending")
  failures[0]?.()
  expect(session.getStatus().phase).toBe("downloading")
})

it.each(["{", "{}"])(
  "a hash-verified unreadable manifest reports a load error (%s)",
  async (text) => {
    const served = origin()
    const bytes = new TextEncoder().encode(text)
    const hash = digest(bytes)
    const path = `snapshots/manifests/${hash.slice(7)}.json`
    served.files.set(path, bytes)
    const fetcher: Fetcher = (url, init) =>
      url.endsWith("snapshots/preview/current.json")
        ? Promise.resolve(
            new Response(canonical({ manifest_path: path, manifest_sha256: hash }).slice().buffer),
          )
        : served.fetcher(url, init)
    const h = harness({ ...served, fetcher })
    h.session.load(base, "jp", "preview")
    await h.settled()
    expect(h.status()).toMatchObject({ phase: "error", generation: null })
    expect(h.events.some((event) => event.kind === "ready")).toBe(false)
  },
)

it("an index entry that differs from its manifest is an error, also for a stored image", async () => {
  const storage = memoryCache()
  const served = origin()
  const first = harness(served, storage)
  first.session.load(base, "jp", "index")
  await first.settled()
  expect(first.status()).toMatchObject({ phase: "ready" })
  const path = "snapshots/versions/index.json"
  const index = objectValue(
    JSON.parse(new TextDecoder().decode(served.files.get(path))) as JsonObject,
  )
  const current = { ...objectValue(index["current"]), published_at: "2026-09-30T00:00:00Z" }
  served.files.set(path, canonical({ ...index, current }))
  for (const cacheStorage of [storage, memoryCache()]) {
    const h = harness(served, cacheStorage)
    h.session.load(base, "jp", "index")
    await h.settled()
    if (cacheStorage === storage) {
      expect(h.status()).toMatchObject({ phase: "ready", unconfirmed: true })
      expect(h.status().error?.message).toContain("revision")
    } else {
      expect(h.status()).toMatchObject({ phase: "error", generation: null })
      expect(h.events.some((event) => event.kind === "ready")).toBe(false)
    }
  }
})

it("set filters follow the printing owner dictionary and open a matching printing", async () => {
  const served = origin()
  const client = createSnapshotClient(base, { fetch: served.fetcher })
  await client.load()
  await client.prefetchPrintings?.()
  await client.selectRegion?.("en")
  await client.prefetchPrintings?.()
  await client.selectRegion?.("jp")
  await client.prefetchPrintings?.()
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("missing fixture")
  const columns = new BootstrapColumns()
  for (const fragment of snapshot.bootstrap) {
    if (fragment.table !== "printing" && fragment.table !== "card_locator") {
      columns.ingest([fragment])
      continue
    }
    for (const row of fragment.rows)
      columns.ingest([
        {
          ...fragment,
          rows: [{ ...row, home_set_id: row["id"] === "p:bp01-001" ? "set:sd01" : "set:bp01" }],
          value: {
            ...fragment.value,
            owner: { kind: "home_set", id: row["id"] === "p:bp01-001" ? "set:sd01" : "set:bp01" },
          },
        },
      ])
  }
  columns.seal()
  const index = new SearchIndex(columns, "jp")
  const page = index.search({ ...query, state: { ...CARD_QUERY, sets: ["sd01"] } })
  expect(page.total).toBe(1)
  expect(page.items[0]?.printingId).toBe("p:bp01-001")
})

it("staging refuses lookups outside its search projection", async () => {
  const client = createSnapshotClient(base, { fetch: origin().fetcher })
  await client.load()
  await client.prefetchPrintings?.()
  await client.selectRegion?.("en")
  await client.prefetchPrintings?.()
  await client.selectRegion?.("jp")
  await client.prefetchPrintings?.()
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("missing snapshot")
  const columns = new BootstrapColumns()
  columns.ingest(snapshot.bootstrap)
  columns.seal()
  const index = columns.index()
  expect(() => index.product("unknown")).toThrow("unavailable")
  expect(() => index.keyword("unknown")).toThrow("unavailable")
  expect(() => index.support("unknown")).toThrow("unavailable")
  expect(() => index.printingByCardNo("jp", "BP01-001")).toThrow("unavailable")
})

it("typed matching preserves catalog ranking, canonical pagination and summaries in either edition", async () => {
  const served = origin()
  const client = createSnapshotClient(base, { fetch: served.fetcher })
  await client.load()
  await client.prefetchPrintings?.()
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("missing snapshot")
  const catalog = catalogOf(snapshot)
  const texts = new Set([
    "",
    "bp",
    "bp01-5",
    "51",
    "051",
    "BP01 001",
    "ｂｐ０１－００１",
    "does not exist",
    " ",
  ])
  for (const entry of catalog.entries)
    for (const name of [...entry.names, ...entry.aliases]) {
      texts.add(name.text)
      texts.add(name.text.slice(0, 2))
      texts.add(name.text.slice(1))
    }
  for (const edition of ["jp", "en"] as const) {
    const client = createSnapshotClient(base, {
      fetch: served.fetcher,
      region: edition,
      cacheStorage: memoryCache(),
    })
    await client.load()
    await client.prefetchPrintings?.()
    const selectedSnapshot = client.snapshot()
    if (!selectedSnapshot) throw new Error("missing regional snapshot")
    const catalog = catalogOf(selectedSnapshot)
    const h = harness(served)
    h.session.load(base, edition, "preview")
    await h.settled()
    const entries = catalog.entries.flatMap((entry) => {
      const printings = entry.printings.filter((printing) => printing.region === edition)
      if (!printings.length) return []
      const preferred = entry.defaultPrinting[edition] ?? printings[0]?.id
      const summary = catalog.summary(preferred ?? "")
      return [
        {
          ...entry,
          printings,
          classCode: summary ? summary.classCode : entry.classCode,
          defaultPrinting: { [edition]: preferred },
        },
      ]
    })
    for (const text of texts) {
      const state = { ...CARD_QUERY, text }
      h.channel.search({ ...query, edition, state, offset: 1, limit: 2 })
      await h.queried()
      const result = h.channel.getStatus().result
      if (result.phase !== "complete") throw new Error("missing result")
      const expected = apply(state, entries, { edition, sets: catalog.sets })
      orderExpected(catalog, expected)
      expect(result.page.total).toBe(expected.length)
      expect(result.page.items).toEqual(
        expected
          .slice(1, 3)
          .map((item) => ({ ...item, summary: catalog.summary(item.printingId) })),
      )
    }
    h.session.dispose()
  }
})

it("a stored index image answers every query like the index it was encoded from", async () => {
  const client = createSnapshotClient(base, { fetch: origin().fetcher })
  await client.load()
  await client.prefetchPrintings?.()
  await client.selectRegion?.("en")
  await client.prefetchPrintings?.()
  await client.selectRegion?.("jp")
  await client.prefetchPrintings?.()
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("missing snapshot")
  const catalog = catalogOf(snapshot)
  const states = [
    CARD_QUERY,
    { ...CARD_QUERY, errataOnly: true },
    ...["bp", "bp01-001", "51", "試作", "does not exist"].map((text) => ({
      ...CARD_QUERY,
      text,
    })),
    ...catalog.entries
      .flatMap((entry) => entry.names)
      .map((name) => ({ ...CARD_QUERY, text: name.text.slice(0, 2) })),
    { ...CARD_QUERY, classes: ["forestcraft"] },
    { ...CARD_QUERY, sets: ["bp01"] },
    { ...CARD_QUERY, cost: [2, 3] },
    { ...CARD_QUERY, attack: [1, 10], defense: [2] },
  ]
  for (const edition of ["jp", "en"] as const) {
    const columns = new BootstrapColumns()
    columns.ingest(snapshot.bootstrap)
    columns.seal()
    const built = new SearchIndex(
      columns,
      edition,
      new Set(
        arrayValue(objectValue(snapshot.manifest["errata_card_ids"])[edition]).map((id) =>
          stringValue(id),
        ),
      ),
    )
    const encoded = built.encode()
    const restored = new SearchIndex(encoded, edition)
    expect(restored.allocation()).toEqual(built.allocation())
    // An image read back at an unaligned offset is copied rather than misread.
    const shifted = new Uint8Array(encoded.length + 1)
    shifted.set(encoded, 1)
    const unaligned = new SearchIndex(shifted.subarray(1), edition)
    for (const state of states)
      for (const page of [{}, { offset: 1, limit: 2 }]) {
        const request = { ...query, ...page, edition, state }
        expect(restored.search(request)).toEqual(built.search(request))
        if (state.errataOnly)
          expect(built.search(request).total).toBe(catalog.results(state, edition).length)
        expect(unaligned.search(request)).toEqual(built.search(request))
      }
    expect(() => new SearchIndex(encoded, edition === "jp" ? "en" : "jp")).toThrow("edition")
  }
})

describe("persistent index images", () => {
  const readyMetrics = (events: SearchEvent[]) =>
    events.flatMap((event) =>
      event.kind === "ready" && event.searchReady !== false ? [event.metrics] : [],
    )

  it.each(["printing", "mechanic"] as const)(
    "does not store an incomplete index when the background %s download fails",
    async (purpose) => {
      const storage = memoryCache()
      const served = origin()
      const payload = (built.manifest["files"] as JsonObject[]).find((file) =>
        purpose === "printing"
          ? file["purpose"] === "product" && file["region"] === "jp"
          : arrayValue(file["row_counts"]).some(
              (count) => arrayValue(count)[0] === "mechanic_projection",
            ),
      )
      if (!payload) throw new Error("background fixture missing")
      const path = stringValue(payload["path"])
      const bytes = served.files.get(path)
      if (!bytes) throw new Error("background payload missing")
      served.files.delete(path)
      const first = harness(served, storage)
      first.session.load(base, "jp", "preview")
      await first.settled()
      expect(first.status()).toMatchObject({ phase: "ready" })
      served.files.set(path, bytes)
      const second = harness(served, storage)
      second.session.load(base, "jp", "preview")
      await second.settled()
      expect(readyMetrics(second.events).slice(-1)).toMatchObject([{ restored: false }])
      expect(second.served.requests).toContain(path)
    },
  )

  it("rebuilds when the manifest, edition or build differs from the stored image", async () => {
    const storage = memoryCache()
    const served = origin()
    const stored = harness(served, storage)
    stored.session.load(base, "jp", "preview")
    await stored.settled()
    const rebuilt = async (h: ReturnType<typeof harness>, edition: "jp" | "en") => {
      h.session.load(base, edition, "preview")
      await h.settled()
      return readyMetrics(h.events)
        .slice(-1)
        .map((metrics) => metrics.restored)
    }
    expect(await rebuilt(harness(served, storage, "other build"), "jp")).toEqual([false])
    expect(await rebuilt(harness(served, storage), "en")).toEqual([false])
    served.update(2)
    expect(await rebuilt(harness(served, storage), "jp")).toEqual([false])
    // The rebuilt image for the new manifest is reused, and the original one is still kept.
    expect(await rebuilt(harness(served, storage), "jp")).toEqual([true])
  })

  it("a damaged image falls back to a full verified build with the same results", async () => {
    const storage = memoryCache()
    const first = harness(origin(), storage)
    first.session.load(base, "jp", "preview")
    await first.settled()
    first.channel.search(query)
    await first.queried()
    const namespace = await storage.open(`sve-search-index:${encodeURIComponent(base)}`)
    for (const request of await namespace.keys()) {
      if (request.url.includes("/plans/")) continue
      const response = await namespace.match(request)
      if (!response) throw new Error("missing image")
      const bytes = new Uint8Array(await response.arrayBuffer())
      const middle = bytes.length >> 1
      bytes[middle] = (bytes[middle] ?? 0) ^ 0xff
      await namespace.put(request, new Response(bytes, { headers: response.headers }))
    }
    const second = harness(origin(), storage)
    second.session.load(base, "jp", "preview")
    second.channel.search(query)
    await second.settled()
    expect(readyMetrics(second.events).slice(-1)).toMatchObject([{ restored: false }])
    await expect.poll(() => completions(second.events), POLL).toBe(1)
    second.channel.search(query)
    await second.queried()
    const before = first.status().result
    const after = second.status().result
    if (before.phase !== "complete" || after.phase !== "complete") throw new Error("missing result")
    expect(after.page).toEqual(before.page)
  })

  it("a custom bootstrap plan never reads or shares stored images", async () => {
    const storage = memoryCache()
    const served = origin()
    const events: SearchEvent[] = []
    const plan: BootstrapPlan = (_manifest, files) =>
      [...files].filter(([, file]) => file["role"] === "bootstrap").map(([key]) => key)
    for (const generation of [1, 2])
      await new SearchEngine((event) => events.push(event), {
        fetch: served.fetcher,
        cacheStorage: storage,
        plan,
        buildId: "build",
      }).handle({ kind: "load", generation, base, edition: "jp", entry: "preview" })
    expect(readyMetrics(events).map((metrics) => metrics.restored)).toEqual([false, false])
  })
})

it("a cached generation replaced by a newer network version re-queries its completed view", async () => {
  const storage = memoryCache()
  const served = origin()
  const first = harness(served, storage)
  first.session.load(base, "jp", "preview")
  await first.settled()
  served.update(2)
  const warm = harness(served, storage)
  warm.channel.view({ ...query, limit: 60 })
  warm.session.load(base, "jp", "preview")
  await warm.settled()
  // The cached view came from the old manifest; the list must follow the version now active.
  expect(warm.status().dataVersion).toBe("20261004T010002Z-0001")
  await expect
    .poll(() => {
      const result = warm.status().result
      return result.phase === "complete" && result.manifestHash === warm.status().manifestHash
    }, POLL)
    .toBe(true)
  const kinds = warm.events.map((event) => event.kind)
  expect(kinds.lastIndexOf("view-result")).toBeGreaterThan(kinds.lastIndexOf("ready"))
})

it("printing pages preserve every edition printing, numbering, filters and cache identity", async () => {
  const served = origin()
  const client = createSnapshotClient(base, {
    fetch: served.fetcher,
    cacheStorage: memoryCache(),
  })
  await client.load()
  await client.prefetchPrintings?.()
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("missing snapshot")
  for (const edition of ["jp", "en"] as const) {
    const client = createSnapshotClient(base, {
      fetch: served.fetcher,
      region: edition,
      cacheStorage: memoryCache(),
    })
    await client.load()
    await client.prefetchPrintings?.()
    const selectedSnapshot = client.snapshot()
    if (!selectedSnapshot) throw new Error("missing regional snapshot")
    const catalog = catalogOf(selectedSnapshot)
    const storage = memoryCache()
    const h = harness(served, storage)
    h.session.load(base, edition, "preview")
    await h.settled()
    const entries = catalog.entries.flatMap((entry) =>
      entry.printings
        .filter((p) => p.region === edition)
        .map((p) => ({ ...entry, printings: [p], defaultPrinting: { [edition]: p.id } })),
    )
    expect(entries.length).toBeGreaterThanOrEqual(
      new Set(entries.map((entry) => entry.cardId)).size,
    )
    for (const text of ["", "BP01", "BP01-001", "51", "試作", "missing"]) {
      const state = { ...CARD_QUERY, unit: "printing" as const, text }
      h.channel.search({ ...query, edition, state, offset: 1, limit: 3 })
      await h.queried()
      const result = h.status().result
      if (result.phase !== "complete") throw new Error("missing printing page")
      const expected = apply(state, entries, { edition, sets: catalog.sets })
      orderExpected(catalog, expected)
      expect(result.page.total).toBe(expected.length)
      expect(result.page.items).toEqual(
        expected.slice(1, 4).map((item) => ({
          key: item.printingId,
          printingId: item.printingId,
          summary: catalog.summary(item.printingId),
        })),
      )
    }
    const state = {
      ...CARD_QUERY,
      unit: "printing" as const,
      classes: ["forestcraft"],
      cost: [0, 1],
    }
    h.channel.search({ ...query, edition, state })
    await h.queried()
    const filtered = h.status().result
    if (filtered.phase !== "complete") throw new Error("missing filtered printings")
    const expected = orderExpected(
      catalog,
      apply(state, entries, { edition, sets: catalog.sets }).filter((item) => {
        const cost = catalog.summary(item.printingId)?.cost
        return cost !== null && cost !== undefined && cost <= 1
      }),
    )
    expect(filtered.page.total).toBe(expected.length)
    expect(filtered.page.items.map((item) => item.printingId)).toEqual(
      expected.map((item) => item.printingId),
    )
    const viewQuery = {
      ...query,
      edition,
      state: { ...CARD_QUERY, unit: "printing" as const },
      limit: 60,
    }
    h.channel.view(viewQuery, true)
    await expect.poll(() => h.status().result.phase, POLL).toBe("complete")
    const complete = h.status().result
    if (complete.phase !== "complete" || !complete.view) throw new Error("missing printing view")
    expect(complete.page.items.every((item) => item.key === item.printingId)).toBe(true)
    expect(complete.view.faces).toEqual([])
    const imageChannel = h.session.createChannel()
    imageChannel.images(
      complete.page.items.map((item) => item.printingId),
      edition,
    )
    await expect.poll(() => imageChannel.getStatus().result.phase, POLL).toBe("complete")
    const fresh = origin()
    const release = fresh.hold("snapshots/preview/current.json")
    const warm = harness(fresh, storage)
    warm.channel.view(viewQuery)
    warm.session.load(base, edition, "preview")
    await expect.poll(() => warm.status().result.phase, POLL).toBe("complete")
    expect(warm.status().result).toMatchObject({ page: complete.page, view: complete.view })
    expect(fresh.requests).toEqual(["snapshots/preview/current.json"])
    release()
    await warm.settled()
    h.session.dispose()
    warm.session.dispose()
  }
})

it.each(["jp", "en"] as const)(
  "retains the navigation order in %s, capped at the originating pages",
  async (edition) => {
    const h = harness()
    h.session.load(base, edition, "preview")
    await h.settled()
    h.channel.search({ ...query, edition })
    await expect.poll(() => h.status().result.phase, POLL).toBe("complete")
    const result = h.status().result
    if (result.phase !== "complete") throw new Error("search pending")
    const items = result.page.items
    const target = items[4]
    if (!target) throw new Error("missing neighbour target")
    h.channel.neighbours({ ...query, edition }, target.key)
    await expect.poll(() => h.status().result.phase, POLL).toBe("complete")
    expect(h.status().result).toMatchObject({ page: { offset: 0, items } })
    h.channel.neighbours({ ...query, edition }, target.key, 5)
    await expect.poll(() => h.status().result.phase, POLL).toBe("complete")
    expect(h.status().result).toMatchObject({ page: { total: 5, items: items.slice(0, 5) } })
    h.session.dispose()
  },
)

it.each(["jp", "en"] as const)(
  "resolves bounded visible effects against the full annotated-text oracle in %s",
  async (edition) => {
    const served = origin()
    const client = createSnapshotClient(base, { fetch: served.fetcher })
    await client.load()
    await client.selectRegion?.(edition)
    const snapshot = client.snapshot()
    if (!snapshot) throw new Error("oracle missing")
    const catalog = catalogOf(snapshot)
    const resolver = sharedAnnotatedTextResolver(client)
    const h = harness(served)
    h.session.load(base, edition, "preview")
    await h.settled()
    h.channel.search({ ...query, edition })
    await expect.poll(() => h.status().result.phase, POLL).not.toBe("pending")
    const page = h.status().result
    if (page.phase !== "complete") throw new Error("list pending")
    const targets = page.page.items
      .slice(0, 8)
      .flatMap((item) => (item.summary ? [item.summary] : []))
    for (const summary of targets)
      for (const language of ["ja", "en", "zh-Hant"]) {
        const revision = catalog.index.displayRevision(summary.faceId, edition)
        const selected = revision
          ? await resolver.resolve(
              {
                owner: { kind: "face_revision", id: stringValue(revision["id"]) },
                field: "effect",
                ordinal: null,
              },
              language,
            )
          : null
        const unit = selected ? (selected.translated ?? selected.original).unit : null
        h.channel.preview(summary.printingId, summary.faceId, language, edition)
        await expect.poll(() => h.status().result.phase, POLL).not.toBe("pending")
        const result = h.status().result
        if (result.phase !== "complete")
          throw new Error(result.phase === "error" ? result.error.message : "preview pending")
        expect(result.preview).toEqual(unit ? { text: unit["text"], language: unit["lang"] } : null)
        expect(result.page.items).toHaveLength(0)
      }
    h.session.dispose()
  },
  30_000,
)

describe("set releases and counts", () => {
  it("retains all composite product families and their first dates across index restore", async () => {
    const client = createSnapshotClient(base, { fetch: origin().fetcher })
    await client.load()
    await client.prefetchPrintings?.()
    await client.selectRegion?.("en")
    await client.prefetchPrintings?.()
    const snapshot = client.snapshot()
    if (!snapshot) throw new Error("missing snapshot")
    const columns = new BootstrapColumns()
    columns.ingest(
      snapshot.bootstrap.map((fragment) =>
        fragment.table === "product"
          ? {
              ...fragment,
              rows: [
                {
                  family_ids: ["set:bp01", "set:sd01"],
                  region: "en",
                  released_on: "2026-05-01",
                  date_precision: "day",
                },
                {
                  family_ids: ["set:bp01"],
                  region: "en",
                  released_on: "2026-02-01",
                  date_precision: "day",
                },
                {
                  family_ids: ["set:sd01"],
                  region: "en",
                  released_on: "2026-08-01",
                  date_precision: "day",
                },
                {
                  family_ids: ["set:bp01", "set:sd01"],
                  region: "jp",
                  released_on: "2026-01-01",
                  date_precision: "day",
                },
              ],
            }
          : fragment,
      ),
    )
    columns.seal()
    const created = new SearchIndex(columns, "en")
    for (const index of [created, new SearchIndex(created.encode(), "en")]) {
      expect(index.releases.find((set) => set.code === "bp01")?.releasedOn).toBe("2026-02-01")
      expect(index.releases.find((set) => set.code === "sd01")?.releasedOn).toBe("2026-05-01")
      expect(index.count({ state: { ...CARD_QUERY, sets: ["bp01"] }, edition: "en" })).toEqual({
        cards: 15,
      })
      expect(index.count({ state: { ...CARD_QUERY, sets: ["sd01"] }, edition: "en" })).toEqual({
        cards: 2,
      })
    }
  })

  it("counts each card of a set once, and returns no cards", async () => {
    const served = origin()
    const baseline = createSnapshotClient(base, { fetch: served.fetcher })
    await baseline.load()
    const snapshot = baseline.snapshot()
    if (!snapshot) throw new Error("missing baseline")
    const catalog = catalogOf(snapshot)
    const family = catalog.index.families.find((row) => row["code"] === "bp01")?.["id"]
    // A printing's set is the owner of the fragment that ships it.
    const owners = new Map(
      snapshot.bootstrap
        .filter((fragment) => fragment.table === "printing")
        .flatMap((fragment) =>
          fragment.rows.map((row) => [row["id"], objectValue(fragment.value["owner"])["id"]]),
        ),
    )
    const cards = catalog.entries.flatMap((entry) => {
      const printing = entry.printings.find(
        (item) => item.region === "jp" && owners.get(item.id) === family,
      )
      const summary = printing && catalog.summary(printing.id)
      return summary ? [summary.name] : []
    })
    const h = harness(served)
    h.session.load(base, "jp", "preview")
    await h.settled()
    const state = { ...CARD_QUERY, sets: ["bp01"] }
    h.channel.count({ state, edition: "jp" })
    await expect.poll(() => h.status().result.phase, POLL).toBe("complete")
    const result = h.status().result
    if (result.phase !== "complete") throw new Error("missing counts")
    expect(result.counts).toEqual({ cards: cards.length })
    expect(result.page).toEqual({ total: cards.length, offset: 0, items: [] })
    // Printings of one card in the set still count it once.
    h.channel.search({ state: { ...state, unit: "printing" }, edition: "jp", limit: 1 })
    await expect.poll(() => h.status().result.phase, POLL).toBe("complete")
    const listed = h.status().result
    if (listed.phase !== "complete") throw new Error("missing printings")
    expect(listed.page.total).toBeGreaterThan(cards.length)
    h.channel.count({ state: { ...state, unit: "printing" }, edition: "jp" })
    await expect.poll(() => h.status().result.phase, POLL).toBe("complete")
    const printings = h.status().result
    if (printings.phase !== "complete") throw new Error("missing counts")
    expect(printings.counts?.cards).toBe(cards.length)
  })
})

// Locator rows lack reprint flags and representative metadata, so these queries wait for printings.
it.each([
  ["lowest rarity", {}, ["p:sd01-002-bp"]],
  ["highest rarity", { representative: "high" as const }, ["p:sd01-002-bp"]],
  // The fixture gives every printing its own art, so the art unit lists both, by number.
  ["same art", { unit: "art" as const }, ["p:sd01-002-bp", "p:sd01-002"]],
  [
    "hidden reprints",
    { unit: "printing" as const, reprints: "hide" as const },
    ["p:sd01-002-bp", "p:sd01-002"],
  ],
])("queries wait for printing completion (%s)", async (_label, changes, expected) => {
  const served = origin()
  const h = harness(served)
  const print = (built.manifest["files"] as JsonObject[]).find(
    (file) => file["purpose"] === "print" && file["region"] === "jp",
  )
  if (!print) throw new Error("print fixture missing")
  const release = served.hold(stringValue(print["path"]))
  // A name query, so every printing of the card is a candidate.
  const state = { ...CARD_QUERY, ...changes, text: "試作の槍兵" }
  h.session.load(base, "jp", "preview")
  h.channel.search({ ...query, state })
  try {
    await expect.poll(() => h.status().phase, POLL).toBe("ready")
    for (let i = 0; i < 5; i += 1) await h.queried()
    expect(h.status().result.phase).toBe("pending")
    expect(h.events.filter((event) => event.kind === "result")).toHaveLength(0)
    release()
    await expect.poll(() => h.status().result.phase, POLL).toBe("complete")
    const result = h.status().result
    if (result.phase !== "complete") throw new Error("missing result")
    // The lowest-numbered printing of this card lives in another set and was a locator until now.
    expect(result.page.items.map((item) => item.printingId)).toEqual(expected)
    expect(result.page.items[0]?.summary?.intId).toBe(1004)
  } finally {
    release()
    h.session.dispose()
  }
})

it("retains badge, Premium and raw dash metadata in compact and restored search summaries", async () => {
  const client = createSnapshotClient(base, { fetch: origin().fetcher })
  await client.load()
  await client.prefetchPrintings?.()
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("missing snapshot")
  const columns = new BootstrapColumns()
  columns.ingest(
    snapshot.bootstrap.map((fragment) =>
      fragment.table === "printing"
        ? {
            ...fragment,
            rows: fragment.rows.map((row) =>
              row["id"] === "p:bp01-001"
                ? {
                    ...row,
                    rarity_code: "pr",
                    premium: true,
                    rarity_badge: { style: "gr", text: "GR" },
                  }
                : row["id"] === "p:bp01-020"
                  ? {
                      ...row,
                      rarity_code: null,
                      rarity_raw: "-",
                      premium: false,
                      rarity_badge: null,
                    }
                  : row,
            ),
          }
        : fragment,
    ),
  )
  columns.seal()
  const builtIndex = new SearchIndex(columns, "jp")
  for (const index of [builtIndex, new SearchIndex(builtIndex.encode(), "jp")]) {
    const page = index.search(query)
    expect(page.items.find((item) => item.printingId === "p:bp01-001")?.summary).toMatchObject({
      rarityCode: "pr",
      premium: true,
      rarityBadge: { style: "gr", text: "GR" },
    })
    expect(page.items.find((item) => item.printingId === "p:bp01-020")?.summary).toMatchObject({
      rarityCode: "-",
      premium: false,
      rarityBadge: null,
    })
  }
})

it("gives GBF appearances their own PR group alongside other rarity filters", async () => {
  const client = createSnapshotClient(base, { fetch: origin().fetcher })
  await client.load()
  await client.prefetchPrintings?.()
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("missing snapshot")
  const raritiesByPrinting: Record<string, JsonObject> = {
    "p:bp01-001": { rarity_code: "pr", rarity_badge: { style: "none" } },
    "p:bp01-020": { rarity_code: null, rarity_raw: "-" },
    "p:bp01-021": {
      rarity_code: "pr",
      premium: false,
      rarity_badge: { style: "gbf", text: "PR" },
    },
    "p:bp01-021-sp": { rarity_code: "-", premium: false },
  }
  const columns = new BootstrapColumns()
  columns.ingest(
    snapshot.bootstrap.map((fragment) =>
      fragment.table === "printing"
        ? {
            ...fragment,
            rows: fragment.rows.map((row) => ({
              ...row,
              ...raritiesByPrinting[stringValue(row["id"])],
            })),
          }
        : fragment,
    ),
  )
  columns.seal()
  const created = new SearchIndex(columns, "jp")
  for (const index of [created, new SearchIndex(created.encode(), "jp")]) {
    const ids = (rarities: readonly string[]) =>
      index
        .search({ ...query, limit: 1000, state: { ...DEFAULT_QUERY, unit: "printing", rarities } })
        .items.map((item) => item.printingId)
    const unrated = ids(["unrated"])
    expect(unrated).toContain("p:bp01-020")
    expect(unrated).toContain("p:bp01-021-sp")
    expect(unrated).not.toContain("p:bp01-001")
    expect(ids(["pr:none"])).toContain("p:bp01-001")
    expect(ids(["pr"])).toContain("p:bp01-021")
    expect(ids(["pr:none"])).not.toContain("p:bp01-021")
    expect(ids(["pr:gbf"])).toEqual(["p:bp01-021"])
    expect(ids(["pr:unknown"])).not.toContain("p:bp01-021")
    expect(index.options.prBadgeStyles).toContain("gbf")
    expect(index.options.rarityCounts["pr:gbf"]).toBe(1)
    expect(ids(["pr:none", "unrated"]).sort()).toEqual([...ids(["pr:none"]), ...unrated].sort())
    expect(index.options.rarityCounts["unrated"]).toBe(unrated.length)
  }
})

it("files PR pack printings without a badge under PR without a badge", async () => {
  const client = createSnapshotClient(base, { fetch: origin().fetcher })
  await client.load()
  const initial = client.snapshot()
  if (!initial) throw new Error("missing initial snapshot")
  await client.prefetchPrintings?.()
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("missing snapshot")
  // Official PR without a printed badge belongs to the PR (No badge) facet in this pack.
  const unrated = { rarity_code: null, rarity_raw: "-", rarity_badge: null, premium: null }
  const source = (
    bootstrap: typeof snapshot.bootstrap,
    promo: JsonObject = { rarity_code: "pr", rarity_raw: "PR", rarity_badge: null },
  ) => {
    const overrides: Record<string, JsonObject> = {
      "p:sd01-001": unrated,
      "p:pr-001": promo,
      "p:bp01-001": { rarity_code: "pr", rarity_badge: { style: "none" } },
    }
    const columns = new BootstrapColumns()
    columns.ingest(
      bootstrap.map((fragment) =>
        fragment.table === "printing"
          ? {
              ...fragment,
              rows: fragment.rows.map((row) =>
                Object.hasOwn(row, "rarity_code")
                  ? { ...row, ...overrides[stringValue(row["id"])] }
                  : row,
              ),
            }
          : fragment,
      ),
    )
    columns.seal()
    return columns
  }
  const columns = source(snapshot.bootstrap)
  const created = new SearchIndex(columns, "jp")
  const completed = new SearchIndex(source(initial.bootstrap), "jp")
  completed.completePrintings(columns)
  for (const index of [created, completed, new SearchIndex(completed.encode(), "jp")]) {
    const ids = (rarities: readonly string[]) =>
      index
        .search({ ...query, limit: 1000, state: { ...DEFAULT_QUERY, unit: "printing", rarities } })
        .items.map((item) => item.printingId)
    for (const rarities of [["pr"], ["pr:none"]]) {
      expect(ids(rarities)).toContain("p:pr-001")
      expect(ids(rarities)).toContain("p:bp01-001")
    }
    expect(ids(["unrated"])).not.toContain("p:pr-001")
    expect(ids(["pr:unknown"])).not.toContain("p:pr-001")
    // A printing outside the PR pack without a rarity stays unrated.
    expect(ids(["unrated"])).toContain("p:sd01-001")
    expect(ids(["pr"])).not.toContain("p:sd01-001")
    expect(index.options.prBadgeStyles).toContain("none")
    const counts = index.options.rarityCounts
    expect(counts["pr:none"]).toBe(ids(["pr:none"]).length)
    expect(counts["pr"]).toBe(ids(["pr"]).length)
    expect(counts["pr"]).toBe(
      index.options.prBadgeStyles.reduce((sum, style) => sum + (counts[`pr:${style}`] ?? 0), 0),
    )
    expect(counts["unrated"]).toBe(ids(["unrated"]).length)
  }
  const withBadge = new SearchIndex(
    source(snapshot.bootstrap, { ...unrated, rarity_badge: { style: "br", text: "BR" } }),
    "jp",
  )
  const badged = withBadge.search({
    ...query,
    state: { ...DEFAULT_QUERY, unit: "printing", rarities: ["pr:br"] },
  })
  expect(badged.items.map((item) => item.printingId)).toContain("p:pr-001")
  expect(withBadge.options.rarityCounts["pr:br"]).toBe(badged.total)
})

it("filters leader frames per printing, intersects rarity and restores the index", async () => {
  const client = createSnapshotClient(base, { fetch: origin().fetcher })
  await client.load()
  await client.prefetchPrintings?.()
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("missing snapshot")
  const expected: { id: string; frame: string; rarity: string }[] = []
  const columns = new BootstrapColumns()
  columns.ingest(
    snapshot.bootstrap.map((fragment) => ({
      ...fragment,
      rows: fragment.rows.map((row) => {
        if (fragment.table === "face_revision") return { ...row, type_code: "leader" }
        if (fragment.table !== "printing" || row["region"] !== "jp") return row
        const frame = expected.length % 2 ? "white" : "gold"
        const rarity = expected.length % 3 ? "br" : "pr"
        expected.push({ id: stringValue(row["id"]), frame, rarity })
        return { ...row, leader_name_frame: frame, rarity_code: rarity }
      }),
    })),
  )
  columns.seal()
  const created = new SearchIndex(columns, "jp")
  for (const index of [created, new SearchIndex(created.encode(), "jp")]) {
    const search = (locations: readonly string[], rarities: readonly string[] = []) =>
      index.search({ ...query, state: { ...DEFAULT_QUERY, locations, rarities }, limit: 100 })
    expect(search(["leader"]).total).toBe(expected.length)
    expect(search(["leader:gold", "leader:white"]).total).toBe(expected.length)
    expect(
      search(["leader:gold"])
        .items.map((item) => item.printingId)
        .sort(),
    ).toEqual(
      expected
        .filter((item) => item.frame === "gold")
        .map((item) => item.id)
        .sort(),
    )
    expect(
      search(["leader:white"], ["pr"])
        .items.map((item) => item.printingId)
        .sort(),
    ).toEqual(
      expected
        .filter((item) => item.frame === "white" && item.rarity === "pr")
        .map((item) => item.id)
        .sort(),
    )
    expect(search(["main"]).total).toBe(0)
    expect(
      index.search({ ...query, state: { ...DEFAULT_QUERY, types: ["follower"] }, limit: 100 })
        .total,
    ).toBe(0)
    expect(index.options.types.map((type) => type.code)).toEqual(["leader"])
    expect(index.options.locationCounts["leader:gold"]).toBe(
      expected.filter((item) => item.frame === "gold").length,
    )
    expect(index.options.locationCounts["main"]).toBeUndefined()
  }
})

it("selects Token by special kind and unions it with followers after index restore", async () => {
  const client = createSnapshotClient(base, { fetch: origin().fetcher })
  await client.load()
  await client.prefetchPrintings?.()
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("missing snapshot")
  // The shared fixture predates Token being a special kind.
  const bootstrap = snapshot.bootstrap.map((fragment) =>
    fragment.table === "face_revision"
      ? {
          ...fragment,
          rows: fragment.rows.map((row) =>
            row["type_code"] === "token" ? { ...row, type_code: "follower" } : row,
          ),
        }
      : fragment,
  )
  const revisions = bootstrap
    .filter((fragment) => fragment.table === "face_revision")
    .flatMap((fragment) => fragment.rows)
  const follower = revisions.find((row) => row["type_code"] === "follower")
  if (!follower) throw new Error("missing follower")
  const tokenFaces = new Set(
    revisions
      .filter(
        (row) =>
          ["spell", "amulet"].includes(stringValue(row["type_code"])) ||
          row["face_id"] === follower["face_id"],
      )
      .map((row) => stringValue(row["face_id"])),
  )
  const followerFaces = new Set(
    revisions
      .filter((row) => row["type_code"] === "follower")
      .map((row) => stringValue(row["face_id"])),
  )
  const spellFaces = new Set(
    revisions
      .filter((row) => row["type_code"] === "spell")
      .map((row) => stringValue(row["face_id"])),
  )
  const basic = new BootstrapColumns()
  basic.ingest(bootstrap)
  basic.seal()
  const originals = new SearchIndex(basic, "jp").search({
    ...query,
    state: DEFAULT_QUERY,
    limit: 1000,
  }).items
  const expectedTokens = originals
    .filter((item) => tokenFaces.has(item.summary?.faceId ?? ""))
    .map((item) => item.printingId)
    .sort()
  const expectedSpells = originals
    .filter((item) => spellFaces.has(item.summary?.faceId ?? ""))
    .map((item) => item.printingId)
    .sort()
  const expectedFollowers = originals
    .filter((item) => followerFaces.has(item.summary?.faceId ?? ""))
    .map((item) => item.printingId)
    .sort()
  expect(expectedTokens.length).toBeGreaterThan(0)
  expect(expectedTokens.some((id) => expectedFollowers.includes(id))).toBe(true)
  const columns = new BootstrapColumns()
  columns.ingest(
    bootstrap.map((fragment) => ({
      ...fragment,
      rows: fragment.rows.map((row) => {
        if (fragment.table === "face_revision")
          return {
            ...row,
            special_kinds: tokenFaces.has(stringValue(row["face_id"])) ? ["token"] : [],
          }
        if (fragment.table === "vocabulary" && row["kind"] === "type" && row["code"] === "token")
          return { ...row, kind: "special_kind" }
        return row
      }),
    })),
  )
  columns.seal()
  const created = new SearchIndex(columns, "jp")
  for (const index of [created, new SearchIndex(created.encode(), "jp")]) {
    const ids = (state: Partial<typeof DEFAULT_QUERY>) =>
      index
        .search({ ...query, state: { ...DEFAULT_QUERY, ...state }, limit: 1000 })
        .items.map((item) => item.printingId)
        .sort()
    const all = ids({})
    expect(ids({ locations: ["token"] })).toEqual(expectedTokens)
    expect(ids({ locations: ["token:spell"] })).toEqual(expectedSpells)
    expect(ids({ locations: ["token:follower", "token:spell", "token:amulet"] })).toEqual(
      expectedTokens,
    )
    // Rows intersect: a Token spell is still a spell, and a follower Token is still a follower.
    expect(ids({ types: ["spell"], locations: ["token"] })).toEqual(expectedSpells)
    expect(ids({ types: ["follower"] })).toEqual(expectedFollowers)
    expect(ids({ types: ["follower"], locations: ["token"] })).toEqual(
      expectedFollowers.filter((id) => expectedTokens.includes(id)),
    )
    // "main" is everything outside the other locations, so every row together covers the fixture
    // and no Token is in the main deck.
    expect(ids({ locations: [...LOCATION_ORDER] })).toEqual(all)
    const main = ids({ locations: ["main"] })
    expect(main.length).toBeGreaterThan(0)
    expect(main.some((id) => expectedTokens.includes(id))).toBe(false)
    expect(index.options.locationCounts["token"]).toBe(expectedTokens.length)
    expect(index.options.locationCounts["main"]).toBe(main.length)
    expect(index.options.typeCounts["token"]).toBeUndefined()
    expect(index.options.types.find((type) => type.code === "token")).toBeUndefined()
  }
})

it("filters evolve and advance by special kind and traits by the card's vocabulary codes", async () => {
  const client = createSnapshotClient(base, { fetch: origin().fetcher })
  await client.load()
  await client.prefetchPrintings?.()
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("missing snapshot")
  // The shared fixture types evolved followers instead of marking the special kind; give every
  // such face the kind, mark one spell as advance, and read the traits the fixture already has.
  const evolveFaces = new Set<string>(),
    advanceFaces = new Set<string>()
  const traitFaces = new Map<string, string[]>()
  const uncreditedArts = new Set<string>()
  const hashed = (text: string) =>
    Array.from(text.matchAll(/[\s\S]/gu)).reduce((sum, char) => sum + char[0].charCodeAt(0), 0)
  const columns = new BootstrapColumns()
  columns.ingest(
    snapshot.bootstrap.map((fragment) => ({
      ...fragment,
      rows: fragment.rows.map((row) => {
        // Every third art loses its credits, so some printings are uncredited.
        if (fragment.table === "art") {
          const lost = hashed(stringValue(row["id"])) % 3 === 0
          if (lost) uncreditedArts.add(stringValue(row["id"]))
          return lost ? { ...row, artists: [] } : row
        }
        if (fragment.table !== "face_revision") return row
        const face = stringValue(row["face_id"])
        traitFaces.set(
          face,
          arrayValue(row["traits"] ?? []).map((value) => stringValue(value)),
        )
        if (row["type_code"] === "evolved") {
          evolveFaces.add(face)
          return { ...row, type_code: "follower", special_kinds: ["evolve"] }
        }
        if (row["type_code"] === "spell" && advanceFaces.size === 0) {
          advanceFaces.add(face)
          return { ...row, special_kinds: ["advance"] }
        }
        return row
      }),
    })),
  )
  columns.seal()
  const created = new SearchIndex(columns, "jp")
  for (const index of [created, new SearchIndex(created.encode(), "jp")]) {
    const page = (state: Partial<typeof DEFAULT_QUERY>) =>
      index.search({ ...query, state: { ...DEFAULT_QUERY, ...state }, limit: 1000 }).items
    const ids = (state: Partial<typeof DEFAULT_QUERY>) =>
      page(state)
        .map((item) => item.printingId)
        .sort()
    const all = page({})
    const faceOf = (printingId: string) =>
      all.find((item) => item.printingId === printingId)?.summary?.faceId ?? ""
    const expectedEvolve = all
      .filter((item) => evolveFaces.has(item.summary?.faceId ?? ""))
      .map((item) => item.printingId)
      .sort()
    const expectedAdvance = all
      .filter((item) => advanceFaces.has(item.summary?.faceId ?? ""))
      .map((item) => item.printingId)
      .sort()
    expect(expectedEvolve.length).toBeGreaterThan(0)
    expect(expectedAdvance.length).toBeGreaterThan(0)
    expect(ids({ locations: ["evolve"] })).toEqual(expectedEvolve)
    expect(ids({ locations: ["advance"] })).toEqual(expectedAdvance)
    expect(ids({ locations: ["evolve", "advance"] })).toEqual(
      [...expectedEvolve, ...expectedAdvance].sort(),
    )
    // Evolved followers are followers for the type row, and never in the main deck.
    expect(ids({ types: ["follower"], locations: ["evolve"] })).toEqual(expectedEvolve)
    expect(ids({ types: ["spell"], locations: ["evolve"] })).toEqual([])
    expect(ids({ locations: ["main"] }).some((id) => expectedEvolve.includes(id))).toBe(false)
    expect(index.options.locationCounts["evolve"]).toBe(expectedEvolve.length)
    expect(index.options.locationCounts["advance"]).toBe(expectedAdvance.length)

    const traits = index.options.traits.map((trait) => trait.code)
    expect(traits.length).toBeGreaterThan(1)
    for (const trait of traits) {
      const expected = all
        .filter((item) => traitFaces.get(item.summary?.faceId ?? "")?.includes(trait))
        .map((item) => item.printingId)
        .sort()
      expect(expected.length, trait).toBeGreaterThan(0)
      expect(ids({ traits: [trait] }), trait).toEqual(expected)
      expect(index.options.traitCounts[trait], trait).toBe(expected.length)
    }
    const [first = "", second = ""] = traits
    expect(ids({ traits: [first, second] })).toEqual(
      all
        .filter((item) => {
          const codes = traitFaces.get(faceOf(item.printingId)) ?? []
          return codes.includes(first) || codes.includes(second)
        })
        .map((item) => item.printingId)
        .sort(),
    )
    expect(ids({ traits: ["synthetic-missing-trait"] })).toEqual([])
    expect(index.options.traits.every((trait) => trait.name.original.text !== "")).toBe(true)
    // Artists follow the printing's face arts, and only the edition's own credits count.
    const credits = new Map<string, Set<string>>()
    for (const fragment of snapshot.bootstrap.filter((item) => item.table === "art"))
      for (const row of fragment.rows)
        credits.set(
          stringValue(row["id"]),
          new Set(
            arrayValue(uncreditedArts.has(stringValue(row["id"])) ? [] : (row["artists"] ?? []))
              .filter((credit) => (credit as JsonObject)["role"] === "illustration_jp")
              .map((credit) => stringValue((credit as JsonObject)["artist_id"])),
          ),
        )
    const artistIds = index.options.artists.map((artist) => artist.id)
    expect(artistIds.length).toBeGreaterThan(1)
    for (const artist of artistIds) {
      const expected = all
        .filter((item) =>
          arrayValue(
            snapshot.bootstrap
              .find(
                (f) => f.table === "printing" && f.rows.some((r) => r["id"] === item.printingId),
              )
              ?.rows.find((r) => r["id"] === item.printingId)?.["faces"] ?? [],
          ).some((face) => credits.get(stringValue((face as JsonObject)["art_id"]))?.has(artist)),
        )
        .map((item) => item.printingId)
        .sort()
      expect(expected.length, artist).toBeGreaterThan(0)
      expect(ids({ artists: [artist] }), artist).toEqual(expected)
      expect(index.options.artistCounts[artist], artist).toBe(expected.length)
    }
    // The reserved "uncredited" choice matches printings nobody credits, alone or mixed (OR).
    const creditedBy = (printingId: string, artist: string) =>
      arrayValue(
        snapshot.bootstrap
          .find((f) => f.table === "printing" && f.rows.some((r) => r["id"] === printingId))
          ?.rows.find((r) => r["id"] === printingId)?.["faces"] ?? [],
      ).some((face) => credits.get(stringValue((face as JsonObject)["art_id"]))?.has(artist))
    const uncredited = all
      .filter((item) => !artistIds.some((artist) => creditedBy(item.printingId, artist)))
      .map((item) => item.printingId)
      .sort()
    expect(uncredited.length).toBeGreaterThan(0)
    expect(ids({ artists: [UNCREDITED_ARTIST] })).toEqual(uncredited)
    expect(index.options.artistCounts[UNCREDITED_ARTIST]).toBe(uncredited.length)
    expect(index.options.artists.some((artist) => artist.id === UNCREDITED_ARTIST)).toBe(false)
    // "Credited" is every artist at once: all credited printings, and with "uncredited" everything.
    const credited = all.filter((item) => !uncredited.includes(item.printingId))
    expect(ids({ artists: [CREDITED_ARTIST] })).toEqual(
      credited.map((item) => item.printingId).sort(),
    )
    expect(ids({ artists: artistIds })).toEqual(ids({ artists: [CREDITED_ARTIST] }))
    expect(index.options.artistCounts[CREDITED_ARTIST]).toBe(credited.length)
    expect(ids({ artists: [CREDITED_ARTIST, UNCREDITED_ARTIST] })).toEqual(ids({}))
    expect(index.options.artists.some((artist) => artist.id === CREDITED_ARTIST)).toBe(false)
    const [someArtist = ""] = artistIds
    expect(ids({ artists: [someArtist, UNCREDITED_ARTIST] })).toEqual(
      [...new Set([...ids({ artists: [someArtist] }), ...uncredited])].sort(),
    )
    expect(ids({ artists: ["synthetic-missing-artist"] })).toEqual([])
  }
})

it("does not infer uncredited matches when a snapshot has no artist credits", async () => {
  const client = createSnapshotClient(base, { fetch: origin().fetcher })
  await client.load()
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("missing snapshot")
  const columns = new BootstrapColumns()
  columns.ingest(
    snapshot.bootstrap.map((fragment) => ({
      ...fragment,
      rows: fragment.rows.map((row) => (fragment.table === "art" ? { ...row, artists: [] } : row)),
    })),
  )
  columns.seal()

  const created = new SearchIndex(columns, "jp")
  const restored = new SearchIndex(created.encode(), "jp")
  for (const index of [created, restored]) {
    expect(index.options.artistCounts[UNCREDITED_ARTIST]).toBeUndefined()
    expect(
      index.search({
        ...query,
        state: { ...DEFAULT_QUERY, artists: [UNCREDITED_ARTIST] },
      }).items,
    ).toEqual([])
  }

  created.completePrintings(columns)
  expect(created.options.artistCounts[UNCREDITED_ARTIST]).toBeUndefined()
  expect(
    created.search({
      ...query,
      state: { ...DEFAULT_QUERY, artists: [UNCREDITED_ARTIST] },
    }).items,
  ).toEqual([])
})

it.each(["keyword", "ability", "class", "collab"] as const)(
  "keeps a %s query pending until its independent closure completes",
  async (kind) => {
    const engineFile = (built.manifest["files"] as JsonObject[]).find((file) =>
      arrayValue(file["row_counts"]).some(
        (count) => arrayValue(count)[0] === "mechanic_projection",
      ),
    )
    if (!engineFile) throw new Error("missing mechanic fixture")
    const h = harness()
    const release = h.served.hold(stringValue(engineFile["path"]))
    h.session.load(base, "jp", "preview")
    await expect.poll(() => h.status().phase, POLL).toBe("ready")
    h.channel.search({
      ...query,
      state: {
        ...DEFAULT_QUERY,
        ...(kind === "keyword"
          ? { mechanics: ["kw:ward"] }
          : kind === "ability"
            ? { abilityTypes: ["fanfare" as const] }
            : kind === "class"
              ? { classMechanisms: ["combo" as const] }
              : { collabMechanisms: ["serve" as const] }),
      },
    })
    await h.queried()
    expect(h.status().result.phase).toBe("pending")
    release()
    await h.settled()
    const result = h.status().result
    expect(result.phase).toBe("complete")
    if (result.phase !== "complete") throw new Error("header closure did not finish")
    if (kind === "keyword") expect(result.page.total).toBe(3)
    else if (kind === "ability")
      expect(result.page.items.some((item) => item.summary?.cardId === "c:bp01-021")).toBe(true)
    else expect(result.page.total).toBe(0)
    h.session.dispose()
  },
)

it.each(["jp", "en"] as const)(
  "filters only %s announcements before grouping and keeps them in a restored index",
  async (edition) => {
    const cache = memoryCache()
    for (let pass = 0; pass < 2; pass++) {
      const h = harness(origin(), cache)
      h.session.load(base, edition, "preview")
      await h.settled()
      h.channel.search({ ...query, edition, state: { ...CARD_QUERY, errataOnly: true } })
      await h.settled()
      const result = h.status().result
      expect(result.phase).toBe("complete")
      if (result.phase !== "complete") throw new Error("missing result")
      expect(result.page.items.map((item) => item.key).sort()).toEqual(
        edition === "jp" ? ["c:bp01-020", "c:bp01-021"] : [],
      )
      h.session.dispose()
    }
  },
)

it.each(["ep", "sep"])("names and locates a textless unlisted %s printing", async (cardType) => {
  const client = createSnapshotClient(base, { fetch: origin().fetcher })
  await client.load()
  const initial = client.snapshot()
  if (!initial) throw new Error("missing initial snapshot")
  await client.prefetchPrintings?.()
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("missing snapshot")
  const jpPrintings = snapshot.bootstrap
    .filter((fragment) => fragment.table === "printing")
    .flatMap((fragment) => fragment.rows)
    .filter((row) => row["region"] === "jp")
  const perCard = new Map<string, number>()
  for (const row of jpPrintings)
    perCard.set(stringValue(row["card_id"]), (perCard.get(stringValue(row["card_id"])) ?? 0) + 1)
  // A card with one printing, so rewriting it leaves no listed printing behind.
  const target = jpPrintings.find((row) => perCard.get(stringValue(row["card_id"])) === 1)
  if (!target) throw new Error("no single-printing card in the fixture")
  const unit = { id: "t:manual-unlisted", lang: "ja", text: "ゆいっこ" }
  const rewrite = (source: readonly (typeof snapshot.bootstrap)[number][]) => {
    const columns = new BootstrapColumns()
    const textUnits = source.find((fragment) => fragment.table === "text_unit")
    if (!textUnits) throw new Error("no text unit fragment")
    columns.ingest([
      ...source.map((fragment) => ({
        ...fragment,
        rows: fragment.rows.map((row) => {
          if (fragment.table === "printing" && row["id"] === target["id"])
            return {
              ...row,
              card_no: "",
              card_no_state: "none",
              catalog_state: "unlisted",
              manual_available_on: null,
              manual_date_precision: "unknown",
              printed_code: "2024千葉-A",
              manual_name_unit_id: unit.id,
              manual_type_code: cardType,
            }
          // No observation, so no current wording and no display revision.
          if (fragment.table === "face" && row["card_id"] === target["card_id"])
            return { ...row, current: [], wording: [] }
          return row
        }),
      })),
      { ...textUnits, identity: "synthetic-manual-name", rows: [unit] },
    ])
    columns.seal()
    return columns
  }
  const created = new SearchIndex(rewrite(snapshot.bootstrap), "jp")
  const originalColumns = new BootstrapColumns()
  originalColumns.ingest(snapshot.bootstrap)
  originalColumns.seal()
  const original = new SearchIndex(originalColumns, "jp")
  const originalType = original.search({
    ...query,
    state: { ...DEFAULT_QUERY, text: stringValue(target["card_no"]) },
  }).items[0]?.summary?.typeCode
  const restored = new SearchIndex(created.encode(), "jp")
  // A catalog locator knows no marks yet; completing it from the rewritten rows sets the filter.
  const locators = new BootstrapColumns()
  locators.ingest(initial.bootstrap)
  locators.seal()
  const completed = new SearchIndex(locators, "jp")
  completed.completePrintings(rewrite(snapshot.bootstrap))
  for (const index of [created, restored, completed, new SearchIndex(completed.encode(), "jp")]) {
    const search = (state: Partial<typeof DEFAULT_QUERY>) =>
      index.search({ ...query, limit: 1000, state: { ...DEFAULT_QUERY, ...state } })
    expect(search({ unlistedOnly: true }).items.map((item) => item.printingId)).toEqual([
      target["id"],
    ])
    expect(
      search({ unlistedOnly: true, locations: [cardType] }).items.map((item) => item.printingId),
    ).toEqual([target["id"]])
    expect(search({ unlistedOnly: true, locations: ["main"] }).items).toEqual([])
    expect(search({ unlistedOnly: true, types: [cardType] }).items).toHaveLength(1)
    expect(search({ unlistedOnly: true }).items[0]?.summary?.typeCode).toBe(cardType)
    expect(index.options.typeCounts[cardType]).toBe(
      (original.options.typeCounts[cardType] ?? 0) + 1,
    )
    expect(index.options.locationCounts[cardType]).toBe(
      (original.options.locationCounts[cardType] ?? 0) + 1,
    )
    expect(index.options.locationCounts["main"]).toBe(
      (original.options.locationCounts["main"] ?? 0) - 1,
    )
    if (originalType)
      expect(index.options.typeCounts[originalType]).toBe(
        (original.options.typeCounts[originalType] ?? 0) - 1,
      )
    expect(search({}).items.length).toBeGreaterThan(1)
    // Ticking it with another mark keeps only printings that match both.
    expect(search({ unlistedOnly: true, stampOnly: true }).items).toEqual([])
    // The printed code is searchable text: exact (separators and width ignored), prefix, infix.
    for (const text of ["2024千葉-A", "２０２４千葉ａ", "2024千葉", "千葉-A"])
      expect(
        search({ text, unit: "printing" }).items.map((item) => item.printingId),
        text,
      ).toContain(target["id"])
    expect(search({ text: "2024千葉-A" }).items[0]?.printingId).toBe(target["id"])
    expect(search({ text: "2024千葉-B" }).items.map((item) => item.printingId)).not.toContain(
      target["id"],
    )
    // A suggestion reads it as the number slot the cell shows, best first when exact.
    expect(index.suggest("2024千葉-A").items[0]).toMatchObject({
      summary: { printingId: target["id"] },
      field: "cardNo",
      rank: 0,
    })
    expect(
      index.suggest("2024千葉").items.find((item) => item.summary.printingId === target["id"]),
    ).toMatchObject({ field: "cardNo", rank: 1 })
  }
  // An unlisted printing is a card's default one, so the first build already names it.
  for (const index of [created, restored]) {
    const search = (state: Partial<typeof DEFAULT_QUERY>) =>
      index.search({ ...query, limit: 1000, state: { ...DEFAULT_QUERY, ...state } })
    expect(search({ unlistedOnly: true }).items[0]?.summary).toMatchObject({
      cardNo: "",
      cardNoState: "none",
      unlisted: true,
      printedCode: "2024千葉-A",
      name: { original: { lang: "ja", text: "ゆいっこ" } },
    })
    expect(search({ text: "ゆいっこ" }).items.map((item) => item.printingId)).toEqual([
      target["id"],
    ])
  }
})

it("groups numberless unlisted printings by home set before comparing int_id", async () => {
  const client = createSnapshotClient(base, { fetch: origin().fetcher })
  await client.load()
  const initial = client.snapshot()
  if (!initial) throw new Error("missing initial snapshot")
  await client.prefetchPrintings?.()
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("missing snapshot")
  const rows = snapshot.bootstrap
    .filter((f) => f.table === "printing")
    .flatMap((f) => f.rows)
    .filter((row) => row["region"] === "jp")
  const counts = new Map<string, number>()
  for (const row of rows)
    counts.set(stringValue(row["card_id"]), (counts.get(stringValue(row["card_id"])) ?? 0) + 1)
  const targets = rows
    .filter((row) => counts.get(stringValue(row["card_id"])) === 1)
    .sort((a, b) => stringValue(a["id"]).localeCompare(stringValue(b["id"])))
    .slice(0, 3)
  expect(targets).toHaveLength(3)
  const ids = targets.map((row) => stringValue(row["id"]))
  const intIds = [55306, 55307, 55000]
  const text = { id: "t:sort-manual", lang: "ja", text: "合成EP" }
  // Real unlisted EP/SEP have no stats, so cost ties must fall back to the int_id order.
  const faces = new Set(
    snapshot.bootstrap
      .filter((fragment) => fragment.table === "face")
      .flatMap((fragment) => fragment.rows)
      .filter((row) => targets.some((target) => target["card_id"] === row["card_id"]))
      .map((row) => stringValue(row["id"])),
  )
  const synthetic = (fragments: typeof snapshot.bootstrap) =>
    fragments.map((fragment) => ({
      ...fragment,
      rows: fragment.rows.map((row) => {
        const ordinal = fragment.table === "printing" ? ids.indexOf(stringValue(row["id"])) : -1
        if (fragment.table === "face_revision" && faces.has(stringValue(row["face_id"])))
          return { ...row, cost: null, attack: null, defense: null }
        if (
          fragment.table === "face" &&
          targets.some((target) => target["card_id"] === row["card_id"])
        )
          return { ...row, current: [], wording: [] }
        return fragment.table === "printing" && ordinal >= 0
          ? {
              ...row,
              card_no: "",
              card_no_state: "none",
              catalog_state: "unlisted",
              printed_code: ordinal === 0 ? "2025京都-A" : ordinal === 1 ? "2025京都-B" : null,
              manual_available_on:
                ordinal === 0 ? "2025-01-25" : ordinal === 1 ? "2025-01-26" : null,
              manual_date_precision: ordinal < 2 ? "day" : "unknown",
              int_id: integerValue(intIds[ordinal]),
              manual_name_unit_id: text.id,
              manual_type_code: "ep",
            }
          : row
      }),
    }))
  const columns = new BootstrapColumns()
  columns.ingest(synthetic(snapshot.bootstrap))
  const textFragment = snapshot.bootstrap.find((fragment) => fragment.table === "text_unit")
  if (!textFragment) throw new Error("missing text units")
  columns.ingest([{ ...textFragment, identity: "sort-manual-name", rows: [text] }])
  columns.seal()
  const created = new SearchIndex(columns, "jp")
  const locators = new BootstrapColumns()
  locators.ingest(synthetic(initial.bootstrap))
  locators.ingest([{ ...textFragment, identity: "sort-manual-name", rows: [text] }])
  locators.seal()
  const completed = new SearchIndex(locators, "jp")
  completed.completePrintings(columns)
  const originalColumns = new BootstrapColumns()
  originalColumns.ingest(snapshot.bootstrap)
  originalColumns.seal()
  const original = new SearchIndex(originalColumns, "jp")
  const expected = [ids[2], ids[0], ids[1]]
  for (const index of [
    created,
    new SearchIndex(created.encode(), "jp"),
    completed,
    new SearchIndex(completed.encode(), "jp"),
  ]) {
    for (const unit of ["printing", "card", "art"] as const) {
      const search = (state: Partial<SearchQuery["state"]>) =>
        index
          .search({ ...query, limit: 10000, state: { ...ascendingQuery, unit, ...state } })
          .items.map((item) => item.printingId)
      expect(search({ unlistedOnly: true, sort: "no" })).toEqual(expected)
      expect(search({ unlistedOnly: true, sort: "no", order: "desc" })).toEqual(
        [...expected].reverse(),
      )
      expect(search({ unlistedOnly: true, sort: "cost" })).toEqual(expected)
      expect(search({ unlistedOnly: true, sort: "set" })).toEqual(expected)
      expect(search({ unlistedOnly: true, sort: "set", order: "desc" })).toEqual(expected)
      expect(
        search({ unlistedOnly: true, sort: "set", order: "desc", secondaryOrder: "desc" }),
      ).toEqual([...expected].reverse())
      expect(
        search({ unlistedOnly: true, sort: "set", order: "desc", secondarySort: null }),
      ).toEqual(expected)
      expect(search({ unlistedOnly: true, text: "2025京都" })).toEqual([ids[0], ids[1]])
    }
    const official = (value: SearchIndex) =>
      value
        .search({
          ...query,
          limit: 10000,
          state: { ...DEFAULT_QUERY, unit: "printing" },
        })
        .items.map((item) => item.printingId)
        .filter((id) => !ids.includes(id))
    expect(official(index)).toEqual(official(original))
  }
})

it("sorts home sets by regional release, groups undated products and PR, and applies independent secondary directions", async () => {
  const client = createSnapshotClient(base, { fetch: origin().fetcher })
  await client.load()
  await client.prefetchPrintings?.()
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("missing fixture")
  const source = new BootstrapColumns()
  source.ingest(snapshot.bootstrap)
  source.seal()
  const ids = new SearchIndex(source, "jp")
    .search(query)
    .items.slice(0, 12)
    .map((item) => item.printingId)
  expect(ids).toHaveLength(12)
  const cases = [
    ["bp21", "BP21-001"],
    ["bp22", "BP22-002"],
    ["bp22", "BP22-001"],
    ["pr", "PR-010"],
    ["pr", "PR-002"],
    ["gfe01", "GFE01-001"],
    ["ny2024", "NY2024-001"],
    ["zz22", "ZZ22-001"],
    ["bp21", "BP22-001"],
    ["bsf2024", "BSF2024-001"],
    ["bsf2025", "BSF2025-001"],
    ["bp21", "BP99-001"],
  ] as const
  const family = snapshot.bootstrap.find((fragment) => fragment.table === "product_family")
  const product = snapshot.bootstrap.find((fragment) => fragment.table === "product")
  if (!family || !product || !family.rows[0]) throw new Error("missing product fixture")
  const codes = [...new Set(cases.map(([set]) => set))]
  const columns = new BootstrapColumns()
  columns.ingest(
    snapshot.bootstrap
      .filter((fragment) => !["product", "product_family"].includes(fragment.table))
      .flatMap((fragment) =>
        fragment.table !== "printing"
          ? [fragment]
          : fragment.rows.map((row) => {
              const entry = cases[ids.indexOf(stringValue(row["id"]))]
              return {
                ...fragment,
                value: entry
                  ? { ...fragment.value, owner: { kind: "home_set", id: `sort:${entry[0]}` } }
                  : fragment.value,
                rows: [entry ? { ...row, card_no: entry[1] } : row],
              }
            }),
      ),
  )
  columns.ingest([
    {
      ...family,
      rows: codes.map((code) => ({
        ...family.rows[0],
        id: `sort:${code}`,
        code,
        public_code: code.toUpperCase(),
      })),
    },
    {
      ...product,
      rows: ["bp21", "bp22", "zz22"].flatMap((code) => [
        {
          id: `sort:${code}:jp`,
          family_ids: [`sort:${code}`],
          region: "jp",
          released_on: code === "bp21" ? "2026-01-01" : "2026-02-01",
          date_precision: "day",
        },
        {
          id: `sort:${code}:en`,
          family_ids: [`sort:${code}`],
          region: "en",
          released_on: "2027-01-01",
          date_precision: "day",
        },
      ]),
    },
  ])
  columns.seal()
  const builtIndex = new SearchIndex(columns, "jp")
  for (const index of [builtIndex, new SearchIndex(builtIndex.encode(), "jp")]) {
    for (const unit of ["printing", "art", "card"] as const) {
      const selected = (state: SearchQuery["state"]) =>
        index
          .search({ ...query, state: { ...state, unit } })
          .items.filter((item) => ids.includes(item.printingId))
          .map((item) => ids.indexOf(item.printingId))
      expect(selected(DEFAULT_QUERY)).toEqual([7, 2, 1, 0, 8, 11, 6, 5, 10, 9, 4, 3])
      const { order: _order, ...ascending } = DEFAULT_QUERY
      expect(selected(ascending)).toEqual([0, 8, 11, 2, 1, 7, 9, 10, 5, 6, 4, 3])
      expect(selected({ ...DEFAULT_QUERY, secondaryOrder: "desc" })).toEqual([
        7, 1, 2, 11, 8, 0, 6, 5, 10, 9, 3, 4,
      ])
      expect(selected({ ...DEFAULT_QUERY, secondarySort: null })).toEqual(selected(DEFAULT_QUERY))
      const numberFirst = selected({
        ...ascending,
        sort: "no",
        secondarySort: "set",
        secondaryOrder: "desc",
      })
      expect(numberFirst.indexOf(2)).toBeLessThan(numberFirst.indexOf(8))
      const numberFirstReverse = selected({ ...ascending, sort: "no", secondarySort: "set" })
      expect(numberFirstReverse.indexOf(8)).toBeLessThan(numberFirstReverse.indexOf(2))
      for (const order of ["asc", "desc"] as const) {
        const bySet = selected({
          ...(order === "desc" ? DEFAULT_QUERY : ascending),
          secondarySort: null,
        })
        expect(bySet.slice(-2).sort()).toEqual([3, 4])
      }
    }
  }
})

it.each(["jp", "en"] as const)(
  "ignores a legacy PR URL and does not request the other region for %s queries",
  async (edition) => {
    const h = harness()
    h.session.load(base, edition, "preview")
    const state = parseQuery(new URLSearchParams("pr=1"))
    expect(state).toEqual(DEFAULT_QUERY)
    h.channel.search({ ...query, edition, state })
    await h.settled()
    await expect.poll(() => h.status().result.phase, POLL).toBe("complete")
    const result = h.status().result
    expect(result.phase === "complete" && result.page.total > 0).toBe(true)
    const paths = (built.manifest["files"] as JsonObject[])
      .filter((file) => file["region"] !== null && file["region"] !== edition)
      .map((file) => stringValue(file["path"]))
    expect(paths.length).toBeGreaterThan(0)
    expect(h.served.requests.filter((path) => paths.includes(path))).toEqual([])
    expect(
      h.events.filter((event) => event.kind === "background-ready").map((event) => event.purpose),
    ).toEqual(["printings", "mechanics"])
    h.session.dispose()
  },
)

it.each(["jp", "en"] as const)(
  "filters SVE-exclusive %s cards using only same-card links, across every printing and restored index",
  async (edition) => {
    const client = createSnapshotClient(base, { fetch: origin().fetcher, region: edition })
    await client.load()
    const initial = client.snapshot()
    if (!initial) throw new Error("missing initial snapshot")
    await client.prefetchPrintings?.()
    const snapshot = client.snapshot()
    const template = snapshot?.bootstrap[0]
    if (!snapshot || !template) throw new Error("missing snapshot")
    const links = [
      { card_id: "c:bp01-001", digital_card_id: "d:sv1", relation: "same_card" },
      { card_id: "c:bp01-002", digital_card_id: "d:sv1", relation: "same_character" },
      { card_id: "c:bp01-003", digital_card_id: "d:svwb", relation: "same_card" },
      { card_id: "c:bp01-010", digital_card_id: "d:sv1", relation: "same_character" },
      { card_id: "c:bp01-010", digital_card_id: "d:svwb", relation: "same_card" },
      { card_id: "c:bp01-020", digital_card_id: "d:sv1", relation: "name_only" },
      { card_id: "c:sd01-001", digital_card_id: "d:sv1", relation: "same_name" },
      {
        card_id: "c:bp01-021",
        face_id: "f:bp01-021",
        digital_card_id: "d:svwb",
        relation: "same_card",
      },
    ]
    const full = new BootstrapColumns()
    full.ingest([
      ...snapshot.bootstrap,
      {
        ...template,
        table: "digital_link",
        identity: "synthetic-digital-links",
        value: { ...template.value, purpose: "engine" },
        rows: links,
      },
    ])
    full.seal()
    const local = new BootstrapColumns()
    local.ingest(initial.bootstrap)
    local.seal()
    const completed = new SearchIndex(local, edition)
    completed.completePrintings(full)
    completed.completeMechanics(full)
    completed.completeSveOnly(full)
    const created = new SearchIndex(full, edition)
    const excluded = new Set(["c:bp01-001", "c:bp01-003", "c:bp01-010", "c:bp01-021"])
    for (const index of [
      created,
      new SearchIndex(created.encode(), edition),
      completed,
      new SearchIndex(completed.encode(), edition),
    ]) {
      for (const unit of ["printing", "art", "card"] as const) {
        const search = (change: Partial<typeof DEFAULT_QUERY>) =>
          index.search({
            ...query,
            edition,
            limit: 1000,
            state: { ...DEFAULT_QUERY, unit, ...change },
          })
        const all = search({}).items
        expect(all.some((item) => item.summary?.cardId === "c:bp01-001")).toBe(true)
        expect(search({ sveOnly: true }).items).toEqual(
          all.filter((item) => !excluded.has(item.summary?.cardId ?? "")),
        )
        for (const card of ["c:bp01-002", "c:bp01-020", "c:sd01-001"])
          expect(
            search({ sveOnly: true }).items.some((item) => item.summary?.cardId === card),
          ).toBe(true)
        expect(search({ sveOnly: true, types: ["follower"] }).items).toEqual(
          search({ types: ["follower"] }).items.filter(
            (item) => !excluded.has(item.summary?.cardId ?? ""),
          ),
        )
      }
    }
  },
)

function sveArtOrigin() {
  const served = origin()
  const manifest = structuredClone(built.manifest)
  const added = [
    {
      table: "digital_link",
      purpose: "engine",
      rows: [
        {
          card_id: "c:bp01-002",
          digital_card_id: "d:art-test",
          relation: "same_card",
          review_level: "model_reviewed",
        },
      ],
    },
    {
      table: "digital_card",
      purpose: "engine",
      rows: [{ id: "d:art-test", game: "sv1", official_id: "synthetic" }],
    },
    {
      table: "digital_art_coverage",
      purpose: "print",
      rows: [
        { art_id: "art:bp01-002:0", game: "sv1", state: "unknown", as_of: "2026-11-17" },
        { art_id: "art:bp01-002-alt:0", game: "sv1", state: "not_found", as_of: "2026-11-17" },
      ],
    },
  ].map(({ table, purpose, rows }) => {
    const fields = columns(rowType(table, "detail"))
    const segment = "family-set:bp01:band-0"
    const bytes = canonical({
      format_version: "4.0.0",
      types: {},
      tables: {
        [table]: [
          [
            ["home_set", "set:bp01"],
            "detail",
            segment,
            null,
            rows.map((row) => fields.map((field) => (row as JsonObject)[field] ?? null)),
          ],
        ],
      },
    })
    const file: JsonObject = {
      key: `${purpose}/shared/sve-${table}`,
      role: "text",
      purpose,
      region: null,
      segment,
      path: `snapshots/blobs/${digest(bytes).slice(7)}.json`,
      bytes: bytes.length,
      sha256: digest(bytes),
      row_counts: [[table, "home_set", "set:bp01", "detail", segment, rows.length]],
      dependencies: [],
      source_refs: [],
    }
    served.files.set(stringValue(file["path"]), bytes)
    return file
  })
  manifest["files"] = [...arrayValue(manifest["files"]), ...added]
  served.replaceManifest(manifest)
  const digitalCards = added.find((file) => stringValue(file["key"]).endsWith("digital_card"))
  if (!digitalCards) throw new Error("missing digital cards file")
  return { ...served, digitalPath: stringValue(digitalCards["path"]) }
}

it("loads extra digital-art data only on SVE queries and reuses completed results across reloads", async () => {
  const served = sveArtOrigin()
  const storage = memoryCache()
  const h = harness(served, storage)
  h.session.load(base, "jp", "preview")
  await h.settled()
  h.channel.search(query)
  await expect.poll(() => h.status().result.phase, POLL).toBe("complete")
  expect(served.requests).not.toContain(served.digitalPath)
  const ordinaryRestore = harness(served, storage)
  ordinaryRestore.session.load(base, "jp", "preview")
  await ordinaryRestore.settled()
  expect(served.requests).not.toContain(served.digitalPath)
  ordinaryRestore.session.dispose()
  const release = served.hold(served.digitalPath)
  h.channel.search({ ...query, state: { ...DEFAULT_QUERY, unit: "printing", sveOnly: true } })
  try {
    await expect.poll(() => served.requests.includes(served.digitalPath), POLL).toBe(true)
    expect(h.status().result.phase).toBe("pending")
    const ordinary = h.session.createChannel()
    ordinary.search(query)
    await expect.poll(() => ordinary.getStatus().result.phase, POLL).toBe("complete")
    release()
    await expect.poll(() => h.status().result.phase, POLL).toBe("complete")
    const result = h.status().result
    if (result.phase !== "complete") throw new Error("missing SVE result")
    const linked = result.page.items.filter((item) => item.summary?.cardId === "c:bp01-002")
    expect(linked.map((item) => item.summary?.printingId)).toEqual(["p:bp01-002-alt"])
    const requests = served.requests.length
    h.channel.search({ ...query, state: { ...DEFAULT_QUERY, unit: "printing", sveOnly: true } })
    await expect.poll(() => h.status().result.phase, POLL).toBe("complete")
    expect(served.requests).toHaveLength(requests)
    const restored = harness(served, storage)
    served.requests.length = 0
    restored.session.load(base, "jp", "preview")
    await restored.settled()
    restored.channel.search({
      ...query,
      state: { ...DEFAULT_QUERY, unit: "printing", sveOnly: true },
    })
    await expect.poll(() => restored.status().result.phase, POLL).toBe("complete")
    const next = restored.status().result
    if (next.phase !== "complete") throw new Error("missing restored SVE result")
    expect(next.page).toEqual(result.page)
    expect(served.requests).toEqual(["snapshots/preview/current.json"])
    restored.session.dispose()
  } finally {
    release()
    h.session.dispose()
  }
})

it("lists a completed collab printing by its own name, and a card cell by the card's", async () => {
  const client = createSnapshotClient(base, { fetch: origin().fetcher })
  await client.load()
  const initial = client.snapshot()
  if (!initial) throw new Error("missing initial snapshot")
  await client.prefetchPrintings?.()
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("missing snapshot")
  const unit = { id: "t:ja:synthetic-collab", lang: "ja", text: "合成コラボ名" }
  const textUnits = snapshot.bootstrap.find((fragment) => fragment.table === "text_unit")
  if (!textUnits) throw new Error("no text unit fragment")
  const full = new BootstrapColumns()
  full.ingest([
    ...snapshot.bootstrap.map((fragment) => ({
      ...fragment,
      rows: fragment.rows.map((row) =>
        // Both JP printings print the collab name, so the card cell's representative does too.
        fragment.table === "printing" &&
        ["p:bp01-002", "p:bp01-002-alt"].includes(stringValue(row["id"]))
          ? {
              ...row,
              faces: (row["faces"] as JsonObject[]).map((face) => ({
                ...face,
                name_unit_id: unit.id,
              })),
            }
          : row,
      ),
    })),
    { ...textUnits, identity: "synthetic-collab-name", rows: [unit] },
  ])
  full.seal()
  // A catalog locator has no faces; the printed name arrives with the printing rows.
  const locators = new BootstrapColumns()
  locators.ingest(initial.bootstrap)
  locators.seal()
  const index = new SearchIndex(locators, "jp")
  index.completePrintings(full)
  const items = (unit: "printing" | "card") =>
    index.search({ ...query, limit: 50, state: { ...DEFAULT_QUERY, unit, text: "BP01-002" } }).items
  const printing = items("printing").find((item) => item.printingId === "p:bp01-002-alt")?.summary
  expect(printing?.name.original.text).toBe("合成コラボ名")
  const card = printing?.treatedAs?.original.text
  expect(card).toBeTruthy()
  const cell = items("card").find((item) => item.key === printing?.cardId)?.summary
  expect(cell?.name.original.text).toBe(card)
  expect(cell?.treatedAs).toBeUndefined()
})

it.each(["jp", "en"] as const)(
  "keeps only cards with no printing in the other region in the %s edition",
  async (edition) => {
    const client = createSnapshotClient(base, { fetch: origin().fetcher })
    await client.load()
    await client.selectRegion?.(edition)
    await client.prefetchPrintings?.()
    const snapshot = client.snapshot()
    if (!snapshot) throw new Error("missing snapshot")
    const columns = new BootstrapColumns()
    columns.ingest(snapshot.bootstrap)
    columns.seal()
    const index = new SearchIndex(columns, edition)
    const cards = (regionOnly: boolean) =>
      new Set(
        index
          .search({
            ...query,
            edition,
            limit: 100,
            state: { ...DEFAULT_QUERY, unit: "card", regionOnly },
          })
          .items.map((item) => item.key),
      )
    const only = cards(true)
    expect(only.size).toBeGreaterThan(0)
    expect(only.size).toBeLessThan(cards(false).size)
    for (const card of columns.index().cards)
      expect(only.has(stringValue(card["id"]))).toBe(
        cards(false).has(stringValue(card["id"])) &&
          arrayValue(card["exclusive_regions"] ?? []).includes(edition),
      )
  },
)
