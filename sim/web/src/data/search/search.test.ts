// @vitest-environment node
import { describe, expect, it, vi } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { cardNoKey } from "../../domain/cardNo"
import { DEFAULT_QUERY } from "../../domain/query/model"
import { apply } from "../../domain/search"
import { memoryCache } from "../../test-utils/cache"
import { sharedAnnotatedTextResolver } from "../annotated-text"
import { createCatalog } from "../catalog"
import type { Fetcher } from "../cdn"
import { createSnapshotClient } from "../client"
import {
  canonical,
  type JsonObject,
  type JsonValue,
  objectValue,
  stringValue,
} from "../format-v3/json"
import { digest } from "../format-v3/sha256"
import {
  type QueryFailure,
  type SearchChannel,
  type SearchChannelStatus,
  type SearchPage,
  type SearchQuery,
  SearchSession,
  type SearchStatus,
} from "../index"
const numeric = new Intl.Collator("en", { numeric: true, sensitivity: "base" })
function orderExpected(
  catalog: ReturnType<typeof createCatalog>,
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
import { SearchBlobs } from "./blobs"
import { BootstrapColumns } from "./bootstrap"
import { Columns, StringPool } from "./columns"
import { SearchEngine } from "./engine"
import { SearchIndex } from "./index"
import type { BootstrapPlan, SearchEvent, SearchRequest } from "./messages"
import type { SearchTransportFactory } from "./session"
import { ViewCache } from "./view"

const built = await buildSnapshot({
  encodeImage: ({ seed }) => Promise.resolve(new TextEncoder().encode(String(seed))),
})
const query: SearchQuery = { state: DEFAULT_QUERY, edition: "jp", limit: 100 }
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

describe("complete Worker catalog generations", () => {
  it("keeps only the last input and searches once after the whole download", async () => {
    const h = harness()
    const observed: ReturnType<typeof h.status>[] = []
    h.session.subscribe(() => {
      observed.push(h.status())
    })
    h.channel.subscribe(() => {
      observed.push(h.status())
    })
    const release = h.served.hold(heldPath)
    h.session.load(base, "jp", "preview")
    for (const text of ["first", "second", "bp01-001"])
      h.channel.search({ ...query, state: { ...DEFAULT_QUERY, text } })
    expect(h.status()).toMatchObject({
      phase: "downloading",
      generation: null,
      result: { phase: "pending" },
    })
    expect(h.requests.filter((request) => request.kind === "search")).toHaveLength(0)
    await expect.poll(() => h.served.requests.includes(heldPath), POLL).toBe(true)
    expect(h.status().result.phase).toBe("pending")
    expect(h.events.some((event) => event.kind === "ready")).toBe(false)
    release()
    await h.settled()
    expect(h.requests.filter((request) => request.kind === "search")).toHaveLength(1)
    expect(h.status().lastInput?.state.text).toBe("bp01-001")
    const result = h.status().result
    expect(result.phase).toBe("complete")
    if (result.phase !== "complete") throw new Error("missing result")
    expect(result.page.items[0]?.key).toBe("c:bp01-001")
    expect(h.events.filter((event) => event.kind === "ready")).toHaveLength(1)
    const readyAt = observed.findIndex((status) => status.phase === "ready")
    expect(observed.slice(0, readyAt).every((status) => status.result.phase === "pending")).toBe(
      true,
    )
  })

  it("queries the old generation while updating and reuses unchanged blobs", async () => {
    const h = harness()
    h.session.load(base, "jp", "preview")
    await h.settled()
    h.channel.search(query)
    await h.queried()
    const first = h.status().result
    const blobs = h.served.requests.filter((path) => path.startsWith("snapshots/blobs/"))
    h.served.update(2)
    const release = h.served.hold("snapshots/preview/current.json")
    h.session.load(base, "jp", "preview")
    h.channel.search(query)
    await h.queried()
    expect(h.status()).toMatchObject({
      phase: "ready",
      generation: 1,
      updating: true,
      result: { phase: "complete" },
    })
    release()
    await h.settled()
    expect(h.status()).toMatchObject({
      generation: 2,
      updating: false,
      result: { phase: "complete" },
    })
    expect(h.served.requests.filter((path) => path.startsWith("snapshots/blobs/"))).toEqual(blobs)
    const result = h.status().result
    if (first.phase !== "complete" || result.phase !== "complete") throw new Error("missing result")
    expect(result.page).toEqual(first.page)
  })

  it("failed update leaves the old index searchable", async () => {
    const h = harness()
    h.session.load(base, "jp", "preview")
    await h.settled()
    h.served.update(2)
    h.served.files.clear()
    h.session.load(base, "jp", "preview")
    await h.settled()
    h.channel.search(query)
    await h.queried()
    expect(h.status()).toMatchObject({
      phase: "ready",
      generation: 1,
      updating: false,
      result: { phase: "complete" },
    })
    expect(h.status().error?.message).toContain("HTTP 404")
  })

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

  it("cold/warm queries match the existing catalog and only return the requested page", async () => {
    const served = origin()
    const baseline = createSnapshotClient(base, { fetch: served.fetcher })
    await baseline.load()
    const snapshot = baseline.snapshot()
    if (!snapshot) throw new Error("missing baseline")
    const catalog = createCatalog(snapshot)
    const storage = memoryCache()
    for (let warm = 0; warm < 2; warm += 1) {
      const h = harness(served, storage)
      h.session.load(base, "jp", "preview")
      await h.settled()
      for (const text of ["", "bp01-001", "試作", "テンプラー", "51", "does not exist"]) {
        const state = { ...DEFAULT_QUERY, text }
        h.channel.search({ ...query, state, limit: 2 })
        await h.queried()
        const result = h.status().result
        expect(result.phase).toBe("complete")
        if (result.phase !== "complete") throw new Error("missing result")
        const expected = catalog
          .results(state, "jp")
          .filter((item) => catalog.summary(item.printingId)?.region === "jp")
        if (!text) orderExpected(catalog, expected)
        expect(result.page.total).toBe(expected.length)
        expect(result.page.items.map(({ key, printingId }) => ({ key, printingId }))).toEqual(
          expected.slice(0, 2),
        )
        expect(result.page.items.every((item) => item.summary?.region === "jp")).toBe(true)
      }
    }
  })

  it("cost bounds read the opened printing's summary, and a max of 7 means 7 or more", async () => {
    const served = origin()
    const baseline = createSnapshotClient(base, { fetch: served.fetcher })
    await baseline.load()
    const snapshot = baseline.snapshot()
    if (!snapshot) throw new Error("missing baseline")
    const catalog = createCatalog(snapshot)
    const all = catalog
      .results(DEFAULT_QUERY, "jp")
      .map((item) => catalog.summary(item.printingId))
      .filter((summary) => summary?.region === "jp")
    const h = harness(served)
    h.session.load(base, "jp", "preview")
    await h.settled()
    const totals: number[] = []
    for (const cost of [{ min: 2, max: 3 }, { min: 7 }, { max: 7 }, { max: 1 }]) {
      h.channel.search({ ...query, state: { ...DEFAULT_QUERY, cost } })
      await h.queried()
      const result = h.status().result
      if (result.phase !== "complete") throw new Error("missing result")
      const upper = cost.max !== undefined && cost.max < 7 ? cost.max : undefined
      const expected =
        cost.min === undefined && upper === undefined
          ? all
          : all.filter(
              (summary) =>
                typeof summary?.cost === "number" &&
                (cost.min === undefined || summary.cost >= cost.min) &&
                (upper === undefined || summary.cost <= upper),
            )
      expect(result.page.total).toBe(expected.length)
      totals.push(result.page.total)
    }
    expect(totals[0]).toBeGreaterThan(0)
    expect(totals[0]).toBeLessThan(all.length)
    expect(totals[2]).toBe(all.length)
  })

  it("cache quota failure still completes and reports nonpersistent progress", async () => {
    const h = harness(origin(), memoryCache(true))
    h.session.load(base, "en", "preview")
    h.channel.search({ ...query, edition: "en" })
    await h.settled()
    expect(h.status().result.phase).toBe("complete")
    expect(h.events.findLast((event) => event.kind === "progress")).toMatchObject({
      persistent: false,
    })
  })
})

it("rejects late responses by both queryId and generation", () => {
  let receive: (event: SearchEvent) => void = () => undefined
  const sent: SearchRequest[] = []
  const session = new SearchSession((callback) => {
    receive = callback
    return {
      send: (request) => {
        sent.push(request)
      },
      dispose: () => undefined,
    }
  })
  const channel: SearchChannel = session.createChannel()
  const emptyPage: SearchPage = { total: 0, offset: 0, items: [] }
  const ready = (generation: number): SearchEvent => ({
    kind: "ready",
    generation,
    edition: "jp",
    root: base,
    dataVersion: "test",
    manifestHash: "hash",
    metrics: {
      restored: false,
      files: 1,
      maxRawBytes: 1,
      parseMs: 0,
      buildMs: 0,
      wallMs: 0,
      firstFileMs: 0,
      firstSetMs: 0,
      arrayBuffers: 0,
      stringBytes: 0,
      cards: 0,
      stagingArrayBuffers: 0,
      stagingStringBytes: 0,
    },
  })
  session.load(base, "jp")
  channel.search(query)
  receive(ready(1))
  const previous = sent.findLast((request) => request.kind === "search")
  if (!previous) throw new Error("missing query")
  channel.search({ ...query, state: { ...DEFAULT_QUERY, text: "last" } })
  receive({
    kind: "result",
    generation: 1,
    channelId: channel.id,
    queryId: previous.queryId,
    page: { total: 1000, offset: 0, items: [] },
  })
  expect(channel.getStatus().result.phase).toBe("pending")
  session.load(base, "jp")
  receive(ready(2))
  const current = sent.findLast((request) => request.kind === "search")
  if (!current) throw new Error("missing query")
  receive({
    kind: "result",
    generation: 1,
    channelId: channel.id,
    queryId: current.queryId,
    page: { total: 1000, offset: 0, items: [] },
  })
  expect(channel.getStatus().result.phase).toBe("pending")
  receive({
    kind: "result",
    generation: 2,
    channelId: channel.id,
    queryId: current.queryId,
    page: emptyPage,
  })
  expect(channel.getStatus().result).toMatchObject({ phase: "complete", page: { total: 0 } })
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

it("switching editions waits for the new region instead of returning old-region hits", async () => {
  const h = harness()
  h.session.load(base, "jp", "preview")
  await h.settled()
  const release = h.served.hold("snapshots/preview/current.json")
  h.session.load(base, "en", "preview")
  h.channel.search({ ...query, edition: "en" })
  expect(h.status().result.phase).toBe("pending")
  release()
  await h.settled()
  const result = h.status().result
  if (result.phase !== "complete") throw new Error("missing result")
  expect(result.page.items.every((item) => item.summary?.region === "en")).toBe(true)
})

it("a replaced staging load drains without publishing its generation", async () => {
  const h = harness()
  const release = h.served.hold(heldPath)
  h.session.load(base, "jp", "preview")
  await expect.poll(() => h.served.requests.includes(heldPath), POLL).toBe(true)
  h.session.load(base, "jp", "preview")
  h.channel.search(query)
  release()
  await h.settled()
  expect(
    h.events.filter((event) => event.kind === "ready").map((event) => event.generation),
  ).toEqual([2])
  expect(h.status().generation).toBe(2)
})

it("torn bytes cannot become active, and typing after failure keeps the error explicit", async () => {
  const h = harness()
  h.served.files.set(heldPath, new Uint8Array([1, 2, 3]))
  h.session.load(base, "jp", "preview")
  await h.settled()
  h.channel.search(query)
  await h.queried()
  expect(h.status()).toMatchObject({
    phase: "error",
    generation: null,
    result: { phase: "error" },
  })
  expect(h.events.some((event) => event.kind === "ready")).toBe(false)
  expect(h.served.requests.filter((path) => path === heldPath)).toHaveLength(2)
  h.served.files.set(heldPath, built.files.get(heldPath) ?? new Uint8Array())
  h.session.retry()
  await h.settled()
  expect(h.status().result.phase).toBe("complete")
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

it("rarity queries use the completed printing closure", async () => {
  const h = harness()
  h.session.load(base, "jp", "preview")
  await h.settled()
  h.channel.search({ ...query, state: { ...DEFAULT_QUERY, rarities: ["legend"] } })
  await h.queried()
  const result = h.status().result
  expect(result.phase).toBe("complete")
  if (result.phase !== "complete") throw new Error("missing rarity results")
  const client = createSnapshotClient(base, { fetch: h.served.fetcher })
  await client.load()
  await client.prefetchPrintings?.()
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("missing oracle")
  const catalog = createCatalog(snapshot)
  expect(result.page.total).toBeGreaterThan(0)
  expect(
    result.page.items.every(
      (item) => catalog.index.printing(item.printingId)?.["rarity_code"] === "legend",
    ),
  ).toBe(true)
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
  const page = index.search({ ...query, state: { ...DEFAULT_QUERY, sets: ["sd01"] } })
  expect(page.total).toBe(1)
  expect(page.items[0]?.printingId).toBe("p:bp01-001")
})

it("shares one generation across independently coalesced result and suggestion channels", async () => {
  const h = harness()
  const suggestions = h.session.createChannel()
  h.session.load(base, "jp", "preview")
  h.channel.search(query)
  suggestions.search({ ...query, state: { ...DEFAULT_QUERY, text: "bp01-001" }, limit: 1 })
  await h.settled()
  expect(h.events.filter((event) => event.kind === "ready")).toHaveLength(1)
  expect(suggestions.getStatus().result).toMatchObject({ phase: "complete" })
  const suggestionPage = suggestions.getStatus().result
  if (suggestionPage.phase !== "complete") throw new Error("missing suggestions")
  expect(suggestionPage.page.items).toHaveLength(1)
  expect(suggestionPage.page.items[0]?.key).toBe("c:bp01-001")
  const all = h.channel.getStatus().result
  expect(all).toMatchObject({ phase: "complete", root: base })
  const before = h.events.length
  for (const text of ["b", "bp", "bp01-001"])
    suggestions.search({ ...query, state: { ...DEFAULT_QUERY, text } })
  for (const text of ["missing", "試作"])
    h.channel.search({ ...query, state: { ...DEFAULT_QUERY, text } })
  await h.queried()
  const results = h.events.slice(before).filter((event) => event.kind === "result")
  expect(results).toHaveLength(2)
  expect(new Set(results.map((event) => event.channelId))).toEqual(
    new Set([h.channel.id, suggestions.id]),
  )
  expect(suggestions.getStatus().lastInput?.state.text).toBe("bp01-001")
  expect(h.channel.getStatus().lastInput?.state.text).toBe("試作")
  const after = h.events.length
  suggestions.search(query)
  suggestions.dispose()
  await h.queried()
  expect(h.events.slice(after).filter((event) => event.kind === "result")).toHaveLength(0)
  h.channel.search(query)
  await h.queried()
  expect(h.channel.getStatus().result).toMatchObject({
    phase: "complete",
    page: all.phase === "complete" ? all.page : {},
  })
})

it("query failures are typed and isolated from other channels and load state", async () => {
  const h = harness()
  const other = h.session.createChannel()
  h.session.load(base, "jp", "preview")
  await h.settled()
  for (const [facets, changes] of [
    [["mechanics"], { mechanics: { quick: "has" } }],
    [["sort"], { sort: "date" }],
  ] as const) {
    h.channel.search({ ...query, state: { ...DEFAULT_QUERY, ...changes } })
    other.search(query)
    await h.queried()
    const result = h.channel.getStatus().result
    const failure: QueryFailure | undefined = result.phase === "error" ? result.error : undefined
    expect(failure).toMatchObject({ kind: "unsupported-query", facets })
    expect(other.getStatus().result.phase).toBe("complete")
    expect(h.session.getStatus()).toMatchObject({ phase: "ready", root: base })
    expect(h.session.getStatus().error).toBeUndefined()
    expect(h.session.getStatus()).not.toHaveProperty("metrics")
  }
  h.channel.search(query)
  await h.queried()
  expect(h.channel.getStatus().result.phase).toBe("complete")
})

it("root changes keep the active result's provenance until the new generation is ready", async () => {
  const served = origin()
  const originalFetch = served.fetcher
  served.fetcher = (url, init) => originalFetch(url.replace(`${base}/replacement`, base), init)
  const h = harness(served)
  h.session.load(base, "jp", "preview")
  h.channel.search(query)
  await h.settled()
  const release = h.served.hold("snapshots/preview/current.json")
  h.session.load(`${base}/replacement`, "jp", "preview")
  h.channel.search(query)
  await h.queried()
  expect(h.session.getStatus()).toMatchObject({
    root: base,
    loading: { root: `${base}/replacement` },
  })
  expect(h.channel.getStatus().result).toMatchObject({ phase: "complete", root: base })
  release()
  await h.settled()
  expect(h.session.getStatus().root).toBe(`${base}/replacement`)
  expect(h.channel.getStatus().result).toMatchObject({
    phase: "complete",
    root: `${base}/replacement`,
  })
})

it("persistent cache evicts all but two successful plans per edition without evicting another root", async () => {
  const storage = memoryCache()
  const files = new Map<string, Uint8Array>()
  const fetcher: Fetcher = (url) => Promise.resolve(new Response(files.get(url)?.slice().buffer))
  const file = (root: string, value: string): JsonObject => {
    const bytes = new TextEncoder().encode(value)
    const path = `snapshots/blobs/${digest(bytes).slice(7)}.json`
    files.set(`${root}/${path}`, bytes)
    return { path, bytes: bytes.length, sha256: digest(bytes) }
  }
  const plan = async (root: string, edition: "jp" | "en", values: string[]) => {
    const blobs = new SearchBlobs(root, fetcher, storage)
    const selected = values.map((value) => file(root, value))
    for (const descriptor of selected) await blobs.read(descriptor, new AbortController().signal)
    await blobs.retainPlan(edition, selected)
    return selected.map((descriptor) => `${root}/${stringValue(descriptor["path"])}`)
  }
  const otherRoot = `${base}/other`
  const isolated = await plan(otherRoot, "jp", ["isolated"])
  const old = await plan(base, "jp", ["old", "shared"])
  const en = await plan(base, "en", ["en", "shared"])
  const previous = await plan(base, "jp", ["previous", "shared"])
  const current = await plan(base, "jp", ["current", "shared"])
  // Repeating the same plan must not replace the previous distinct one.
  await plan(base, "jp", ["current", "shared"])
  const cache = await storage.open(`sve-search-blobs:${encodeURIComponent(base)}`)
  expect(await cache.match(old[0] ?? "")).toBeUndefined()
  for (const path of [...previous, ...current, ...en]) expect(await cache.match(path)).toBeDefined()
  const other = await storage.open(`sve-search-blobs:${encodeURIComponent(otherRoot)}`)
  expect(await other.match(isolated[0] ?? "")).toBeDefined()
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

it("compact cells preserve null, signed, large and fractional numbers and nested values", () => {
  const pool = new StringPool(false)
  const columns = new Columns(pool)
  const values: JsonObject[] = [
    {
      id: "id:unique",
      integer: 0xffff_ffff,
      negative: -3,
      large: 2 ** 40,
      fraction: 1.25,
      empty: null,
      boolean: true,
      nested: { array: [null, false, true, -3, 2 ** 40, 1.25, "重複"], text: "重複" },
    },
  ]
  columns.append(values)
  pool.seal()
  expect(columns.rows()).toEqual(values)
})

it("typed matching preserves catalog ranking, canonical pagination and summaries in either edition", async () => {
  const served = origin()
  const client = createSnapshotClient(base, { fetch: served.fetcher })
  await client.load()
  await client.prefetchPrintings?.()
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("missing snapshot")
  const catalog = createCatalog(snapshot)
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
    const catalog = createCatalog(selectedSnapshot)
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
      const state = { ...DEFAULT_QUERY, text }
      h.channel.search({ ...query, edition, state, offset: 1, limit: 2 })
      await h.queried()
      const result = h.channel.getStatus().result
      if (result.phase !== "complete") throw new Error("missing result")
      const expected = apply(state, entries, { edition, sets: catalog.sets })
      if (!text) orderExpected(catalog, expected)
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

it("a cancelled edition switch reports unavailability without blocking other channels", async () => {
  const h = harness()
  h.session.load(base, "jp", "preview")
  await h.settled()
  const release = h.served.hold("snapshots/preview/current.json")
  h.session.load(base, "en", "preview")
  h.channel.search({ ...query, edition: "en" })
  expect(h.channel.getStatus().result.phase).toBe("pending")
  h.session.cancel()
  expect(h.channel.getStatus().result).toMatchObject({
    phase: "error",
    error: { kind: "edition-not-ready" },
  })
  release()
  await h.settled()
  h.channel.search(query)
  await h.queried()
  expect(h.channel.getStatus().result.phase).toBe("complete")
})

it("an edition query is pending exactly while a load for that edition is in flight", async () => {
  const h = harness()
  h.session.load(base, "jp", "preview")
  await h.settled()
  const release = h.served.hold("snapshots/preview/current.json")
  const notReady = { phase: "error", error: { kind: "edition-not-ready" } }
  // A page can ask for the new edition before it starts that edition's download.
  h.channel.search({ ...query, edition: "en" })
  expect(h.channel.getStatus().result).toMatchObject(notReady)
  h.session.load(base, "en", "preview")
  expect(h.channel.getStatus().result.phase).toBe("pending")
  h.session.load(base, "jp", "preview")
  expect(h.channel.getStatus().result).toMatchObject(notReady)
  h.session.load(base, "en", "preview")
  release()
  await h.settled()
  const result = h.channel.getStatus().result
  if (result.phase !== "complete") throw new Error("missing result")
  expect(result.page.items.every((item) => item.summary?.region === "en")).toBe(true)
})

it("cost columns preserve the signed safe integers accepted by the snapshot reader", async () => {
  const client = createSnapshotClient(base, { fetch: origin().fetcher })
  await client.load()
  await client.prefetchPrintings?.()
  await client.selectRegion?.("en")
  await client.prefetchPrintings?.()
  await client.selectRegion?.("jp")
  await client.prefetchPrintings?.()
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("missing snapshot")
  for (const cost of [-3, 2 ** 40]) {
    const columns = new BootstrapColumns()
    columns.ingest(
      snapshot.bootstrap.map((fragment) =>
        fragment.table === "face_revision"
          ? { ...fragment, rows: fragment.rows.map((row) => ({ ...row, cost })) }
          : fragment,
      ),
    )
    columns.seal()
    const index = new SearchIndex(columns, "jp")
    const page = index.search({ ...query, state: { ...DEFAULT_QUERY, cost: { min: cost } } })
    expect(page.total).toBeGreaterThan(0)
    expect(page.items.every((item) => item.summary?.cost === cost)).toBe(true)
    expect(
      index.search({ ...query, state: { ...DEFAULT_QUERY, cost: { min: cost + 1 } } }).total,
    ).toBe(0)
  }
})

it("cache cleanup still reclaims stale plans when a blob write hits quota", async () => {
  const storage = memoryCache()
  let rejectBlobs = false
  const limited = {
    open: async (name: string) => {
      const cache = await storage.open(name)
      const put = cache.put.bind(cache)
      cache.put = (key, response) =>
        rejectBlobs && new Request(key).url.includes("/snapshots/blobs/")
          ? Promise.reject(new Error("quota"))
          : put(key, response)
      return cache
    },
  } as CacheStorage
  const descriptors = ["old", "previous", "current"].map((text) => {
    const bytes = new TextEncoder().encode(text)
    return {
      bytes,
      file: {
        path: `snapshots/blobs/${digest(bytes).slice(7)}.json`,
        bytes: bytes.length,
        sha256: digest(bytes),
      },
    }
  })
  const fetcher: Fetcher = (url) =>
    Promise.resolve(
      new Response(descriptors.find((item) => url.endsWith(item.file.path))?.bytes.slice().buffer),
    )
  for (const [ordinal, { file }] of descriptors.entries()) {
    if (ordinal === 2) rejectBlobs = true
    const blobs = new SearchBlobs(base, fetcher, limited)
    await blobs.read(file, new AbortController().signal)
    await blobs.retainPlan("jp", [file])
    if (ordinal === 2) expect(blobs.persistent).toBe(false)
  }
  const cache = await storage.open(`sve-search-blobs:${encodeURIComponent(base)}`)
  expect(await cache.match(`${base}/${descriptors[0]?.file.path ?? ""}`)).toBeUndefined()
  expect(await cache.match(`${base}/${descriptors[1]?.file.path ?? ""}`)).toBeDefined()
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
  const catalog = createCatalog(snapshot)
  const states = [
    DEFAULT_QUERY,
    ...["bp", "bp01-001", "51", "試作", "does not exist"].map((text) => ({
      ...DEFAULT_QUERY,
      text,
    })),
    ...catalog.entries
      .flatMap((entry) => entry.names)
      .map((name) => ({ ...DEFAULT_QUERY, text: name.text.slice(0, 2) })),
    { ...DEFAULT_QUERY, classes: ["forestcraft"] },
    { ...DEFAULT_QUERY, sets: ["bp01"] },
    { ...DEFAULT_QUERY, cost: { min: 2, max: 3 } },
  ]
  for (const edition of ["jp", "en"] as const) {
    const columns = new BootstrapColumns()
    columns.ingest(snapshot.bootstrap)
    columns.seal()
    const built = new SearchIndex(columns, edition)
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

  it("a reload of the same manifest restores the index without the manifest or any blob", async () => {
    const storage = memoryCache()
    const first = harness(origin(), storage)
    first.session.load(base, "jp", "preview")
    await first.settled()
    first.channel.search(query)
    await first.queried()
    const second = harness(origin(), storage)
    second.session.load(base, "jp", "preview")
    second.channel.search(query)
    await second.settled()
    expect(second.served.requests).toEqual(["snapshots/preview/current.json"])
    const files = readyMetrics(first.events)[0]?.files
    expect(readyMetrics(second.events)).toMatchObject([{ restored: true, files }])
    expect(second.status()).toMatchObject({
      phase: "ready",
      dataVersion: first.status().dataVersion,
      manifestHash: first.status().manifestHash,
      result: { phase: "complete" },
    })
    const before = first.status().result
    const after = second.status().result
    if (before.phase !== "complete" || after.phase !== "complete") throw new Error("missing result")
    expect(after.page).toEqual(before.page)
  })

  it("does not store a catalog-only index when the background printing download fails", async () => {
    const storage = memoryCache()
    const served = origin()
    const product = (built.manifest["files"] as JsonObject[]).find(
      (file) => file["purpose"] === "product" && file["region"] === "jp",
    )
    if (!product) throw new Error("fixture has no product file")
    const path = stringValue(product["path"])
    const bytes = served.files.get(path)
    if (!bytes) throw new Error("product payload missing")
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
  })

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

  it("a load that fails verification stores nothing", async () => {
    const storage = memoryCache()
    const h = harness(origin(), storage)
    h.served.files.set(heldPath, new Uint8Array([1, 2, 3]))
    h.session.load(base, "jp", "preview")
    await h.settled()
    expect(h.status().phase).toBe("error")
    const namespace = await storage.open(`sve-search-index:${encodeURIComponent(base)}`)
    expect(await namespace.keys()).toEqual([])
  })
})

it("suggestions preserve exact printing and alias ranking, and recent summaries have explicit misses", async () => {
  const h = harness()
  h.session.load(base, "jp", "preview")
  await h.settled()
  h.channel.suggest("bp01-51", "jp")
  await expect.poll(() => h.channel.getStatus().result.phase, POLL).toBe("complete")
  const exact = h.channel.getStatus().result
  expect(exact.phase === "complete" && exact.suggestions?.[0]).toMatchObject({
    field: "cardNo",
    rank: 0,
    summary: { cardNo: "BP01-051", region: "jp" },
  })
  h.channel.suggest("テンプラー", "jp")
  await expect.poll(() => h.channel.getStatus().result.phase, POLL).toBe("complete")
  const alias = h.channel.getStatus().result
  expect(alias.phase === "complete" && alias.suggestions?.[0]).toMatchObject({ field: "alias" })
  h.channel.recent(
    [
      "p:does-not-exist",
      ...(exact.phase === "complete"
        ? (exact.suggestions?.map((item) => item.summary.printingId) ?? [])
        : []),
    ],
    "jp",
  )
  await expect.poll(() => h.channel.getStatus().result.phase, POLL).toBe("complete")
  const recent = h.channel.getStatus().result
  expect(recent.phase === "complete" && recent.recent?.[0]).toEqual({
    printingId: "p:does-not-exist",
    state: "not-in-edition",
  })
  expect(recent.phase === "complete" && recent.recent?.[1]?.state).toBe("available")
})

it("a complete cached view and index are usable while the entry is blocked, without reading metadata", async () => {
  const storage = memoryCache()
  const served = origin()
  const first = harness(served, storage)
  first.session.load(base, "jp", "preview")
  await first.settled()
  const metadata = (built.manifest["files"] as JsonObject[]).filter((file) =>
    (file["row_counts"] as JsonValue[][]).some((row) => row[0] === "printing_image"),
  )
  const blobs = await storage.open(`sve-search-blobs:${encodeURIComponent(base)}`)
  for (const file of metadata) {
    await blobs.delete(`${base}/${stringValue(file["path"])}`)
    served.files.delete(stringValue(file["path"]))
  }
  served.requests.length = 0
  const release = served.hold("snapshots/preview/current.json")
  const warm = harness(served, storage)
  warm.channel.view({ ...query, limit: 60 })
  warm.session.load(base, "jp", "preview")
  await expect.poll(() => warm.status().result.phase, POLL).toBe("complete")
  const result = warm.status().result
  if (result.phase !== "complete" || !result.view) throw new Error("view missing")
  expect(result.view.faces).toEqual([])
  expect(
    result.view.faces.every(
      (face) =>
        face.source !== undefined ||
        face.asset["availability"] !== "available" ||
        face.asset["publication_state"] !== "approved",
    ),
  ).toBe(true)
  expect(served.requests).toEqual(["snapshots/preview/current.json"])
  expect(warm.status().unconfirmed).toBe(true)
  await expect.poll(() => warm.status().searchReady, POLL).toBe(true)
  release()
  await warm.settled()
  expect(warm.status().unconfirmed).toBe(false)
})

it("an index-only hit restores before the entry and completes images from verified metadata", async () => {
  const storage = memoryCache()
  const first = harness(origin(), storage)
  first.session.load(base, "jp", "preview")
  await first.settled()
  await storage.delete(`sve-search-view:${encodeURIComponent(base)}`)
  const served = origin()
  const release = served.hold("snapshots/preview/current.json")
  const warm = harness(served, storage)
  warm.channel.view({ ...query, limit: 60 })
  warm.session.load(base, "jp", "preview")
  await expect.poll(() => warm.status().result.phase, POLL).toBe("complete")
  expect(served.requests).toEqual(["snapshots/preview/current.json"])
  release()
  await warm.settled()
})

it("missing views recover and image errors stay separate from usable search results", async () => {
  const storage = memoryCache()
  const served = origin()
  const first = harness(served, storage)
  first.session.load(base, "jp", "preview")
  await first.settled()
  const views = await storage.open(`sve-search-view:${encodeURIComponent(base)}`)
  for (const key of await views.keys()) await views.put(key, new Response("{"))
  const restored = harness(served, storage)
  restored.channel.view({ ...query, limit: 60 })
  restored.session.load(base, "jp", "preview")
  await restored.settled()
  await expect.poll(() => restored.status().result.phase, POLL).toBe("complete")
  const blobs = await storage.open(`sve-search-blobs:${encodeURIComponent(base)}`)
  await storage.delete(`sve-search-view:${encodeURIComponent(base)}`)
  for (const file of built.manifest["files"] as JsonObject[])
    if ((file["row_counts"] as JsonValue[][]).some((row) => row[0] === "printing_image")) {
      await blobs.delete(`${base}/${stringValue(file["path"])}`)
      served.files.set(stringValue(file["path"]), new Uint8Array([0]))
    }
  const broken = harness(served, storage)
  broken.channel.view({ ...query, limit: 60 })
  broken.session.load(base, "jp", "preview")
  await broken.settled()
  await expect.poll(() => broken.status().result.phase, POLL).toBe("complete")
  const result = broken.status().result
  if (result.phase !== "complete") throw new Error("missing search result")
  const images = broken.session.createChannel()
  images.images(
    result.page.items.map((item) => item.printingId),
    "jp",
  )
  await expect.poll(() => images.getStatus().result.phase, POLL).toBe("error")
  expect(broken.status().result.phase).toBe("complete")
})

it("remembers the current completed query for a view-first reload without metadata", async () => {
  const storage = memoryCache()
  const served = origin()
  const first = harness(served, storage)
  first.session.load(base, "jp", "preview")
  await first.settled()
  const current = { ...query, state: { ...DEFAULT_QUERY, text: "bp01-001" }, limit: 60 }
  first.channel.view(current, true)
  await expect.poll(() => first.channel.getStatus().result.phase, POLL).toBe("complete")
  const pointer = await storage.open(`sve-search-view-selection:${encodeURIComponent(base)}`)
  await expect.poll(async () => (await pointer.keys()).length, POLL).toBe(1)
  served.requests.length = 0
  const release = served.hold("snapshots/preview/current.json")
  const warm = harness(served, storage)
  warm.channel.view(current)
  warm.session.load(base, "jp", "preview")
  await expect.poll(() => warm.channel.getStatus().result.phase, POLL).toBe("complete")
  expect(served.requests).toEqual(["snapshots/preview/current.json"])
  release()
  await warm.settled()
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

it("a later check of an unchanged version keeps completed answers and rebuilds nothing", async () => {
  const h = harness()
  h.session.load(base, "jp", "preview")
  await h.settled()
  h.channel.view({ ...query, limit: 60 })
  await expect.poll(() => h.status().result.phase, POLL).toBe("complete")
  const answered = h.status().result
  const phases: string[] = []
  h.channel.subscribe(() => phases.push(h.channel.getStatus().result.phase))
  const generation = h.status().generation ?? 0
  h.served.requests.length = 0
  h.session.load(base, "jp", "preview")
  await h.settled()
  expect(h.served.requests).toEqual(["snapshots/preview/current.json"])
  expect(h.events.findLast((event) => event.kind === "ready")).toMatchObject({
    generation: generation + 1,
    metrics: { restored: true },
  })
  expect(h.status()).toMatchObject({ phase: "ready", generation: generation + 1, updating: false })
  expect(phases).not.toContain("pending")
  expect(h.status().result).toBe(answered)
})

it("switching back to an edition restores its stored index and view instead of rebuilding", async () => {
  const h = harness()
  h.session.load(base, "jp", "preview")
  await h.settled()
  h.session.load(base, "en", "preview")
  await h.settled()
  h.served.requests.length = 0
  const mark = h.events.length
  h.session.load(base, "jp", "preview")
  h.channel.view({ ...query, limit: 60 })
  await h.settled()
  await expect.poll(() => h.status().result.phase, POLL).toBe("complete")
  expect(h.served.requests).toEqual(["snapshots/preview/current.json"])
  const readies = h.events.slice(mark).filter((event) => event.kind === "ready")
  expect(readies.length).toBeGreaterThan(0)
  expect(readies.every((event) => event.metrics.restored)).toBe(true)
  expect(h.status()).toMatchObject({ edition: "jp", searchReady: true, unconfirmed: false })
})

it("an older selection of the other edition cannot make an unchanged entry look rewritten", async () => {
  const storage = memoryCache()
  const served = origin()
  const en = harness(served, storage)
  en.session.load(base, "en", "index")
  await en.settled()
  const path = "snapshots/versions/index.json"
  const index = objectValue(
    JSON.parse(new TextDecoder().decode(served.files.get(path))) as JsonObject,
  )
  served.files.set(path, canonical({ ...index, revision: Number(index["revision"]) + 1 }))
  const jp = harness(served, storage)
  jp.session.load(base, "jp", "index")
  await jp.settled()
  const h = harness(served, storage)
  h.session.load(base, "jp", "index")
  await h.settled()
  h.session.load(base, "en", "index")
  await h.settled()
  expect(h.status()).toMatchObject({ phase: "ready", edition: "en", unconfirmed: false })
  expect(h.status().error).toBeUndefined()
})

it("visible-image channels share metadata while search channels do not request it", async () => {
  const storage = memoryCache()
  const served = origin()
  const first = harness(served, storage)
  first.session.load(base, "jp", "preview")
  await first.settled()
  await storage.delete(`sve-search-view:${encodeURIComponent(base)}`)
  const blobs = await storage.open(`sve-search-blobs:${encodeURIComponent(base)}`)
  const metadata = (built.manifest["files"] as JsonObject[])
    .filter((file) =>
      (file["row_counts"] as JsonValue[][]).some((row) => row[0] === "printing_image"),
    )
    .map((file) => stringValue(file["path"]))
  for (const path of metadata) await blobs.delete(`${base}/${path}`)
  served.requests.length = 0
  const warm = harness(served, storage)
  const options = warm.session.createChannel()
  warm.channel.view({ ...query, limit: 60 })
  options.view({ ...query, limit: 60 })
  warm.session.load(base, "jp", "preview")
  await warm.settled()
  await expect
    .poll(() => [warm.status().result.phase, options.getStatus().result.phase], POLL)
    .toEqual(["complete", "complete"])
  expect(served.requests.filter((path) => metadata.includes(path))).toEqual([])
  const result = warm.status().result
  if (result.phase !== "complete") throw new Error("missing search results")
  const ids = result.page.items.slice(0, 2).map((item) => item.printingId)
  warm.channel.images(ids, "jp")
  options.images(ids, "jp")
  await expect
    .poll(() => [warm.status().result.phase, options.getStatus().result.phase], POLL)
    .toEqual(["complete", "complete"])
  const read = served.requests.filter((path) => metadata.includes(path))
  expect(read.length).toBeGreaterThan(0)
  expect(new Set(read).size).toBe(read.length)
})

it("separates config, bootstrap, index building and visible-image preparation", async () => {
  const h = harness()
  h.session.load(base, "jp", "preview")
  await h.settled()
  const steps = h.events.flatMap((event) => (event.kind === "stage" ? [event.step] : []))
  expect(steps).toEqual(
    expect.arrayContaining(["index", "manifest", "config", "bootstrap", "search"]),
  )
  const progress = h.events.filter((event) => event.kind === "progress")
  const files = built.manifest["files"] as JsonObject[]
  const total = files.filter(
    (file) => file["purpose"] === "catalog" && (file["region"] === null || file["region"] === "jp"),
  ).length
  expect(progress.every((event) => event.total === total)).toBe(true)
  expect(progress.map((event) => event.done)).toEqual(
    Array.from({ length: total + 1 }, (_, i) => i),
  )
  expect(
    h.events.findLastIndex((event) => event.kind === "stage" && event.step === "images"),
  ).toBeLessThan(h.events.findIndex((event) => event.kind === "ready"))
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
    const catalog = createCatalog(selectedSnapshot)
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
      const state = { ...DEFAULT_QUERY, unit: "printing" as const, text }
      h.channel.search({ ...query, edition, state, offset: 1, limit: 3 })
      await h.queried()
      const result = h.status().result
      if (result.phase !== "complete") throw new Error("missing printing page")
      const expected = apply(state, entries, { edition, sets: catalog.sets })
      if (!text) orderExpected(catalog, expected)
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
      ...DEFAULT_QUERY,
      unit: "printing" as const,
      classes: ["forestcraft"],
      cost: { max: 1 },
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
      state: { ...DEFAULT_QUERY, unit: "printing" as const },
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

it("keeps two printings of one card as separate numbered items after index restore", async () => {
  const client = createSnapshotClient(base, { fetch: origin().fetcher })
  await client.load()
  await client.prefetchPrintings?.()
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("missing snapshot")
  const owner = snapshot.bootstrap.find(
    (fragment) =>
      fragment.table === "printing" && fragment.rows.some((row) => row["region"] === "jp"),
  )
  const original = owner?.rows.find((row) => row["region"] === "jp")
  if (!owner || !original) throw new Error("missing test printing")
  const extra = { ...original, id: "p:synthetic-special", int_id: 9999999, card_no: "BP01-P01" }
  const bootstrap = snapshot.bootstrap.map((fragment) =>
    fragment === owner ? { ...fragment, rows: [...fragment.rows, extra] } : fragment,
  )
  const columns = new BootstrapColumns()
  columns.ingest(bootstrap)
  columns.seal()
  const created = new SearchIndex(columns, "jp")
  for (const index of [created, new SearchIndex(created.encode(), "jp")]) {
    const cards = index.search(query)
    const printings = index.search({ ...query, state: { ...DEFAULT_QUERY, unit: "printing" } })
    expect(printings.total).toBe(
      snapshot.bootstrap.flatMap((fragment) =>
        fragment.table === "printing" ? fragment.rows.filter((row) => row["region"] === "jp") : [],
      ).length + 1,
    )
    expect(
      printings.items.filter((item) => item.summary?.cardId === original["card_id"]),
    ).toHaveLength(
      snapshot.bootstrap.flatMap((fragment) =>
        fragment.table === "printing"
          ? fragment.rows.filter(
              (row) => row["region"] === "jp" && row["card_id"] === original["card_id"],
            )
          : [],
      ).length + 1,
    )
    const extraItem = printings.items.find((item) => item.printingId === extra.id)
    expect(extraItem).toMatchObject({ key: extra.id, summary: { cardNo: "BP01-P01" } })
    expect(cards.items.some((item) => item.printingId === extra.id)).toBe(false)
    expect(
      index.search({ ...query, state: { ...DEFAULT_QUERY, unit: "printing", text: "BP01-P01" } })
        .items,
    ).toEqual([extraItem])
  }
})

it.each(["jp", "en"] as const)(
  "returns only the current card and its neighbours in %s, capped at the originating pages",
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
    expect(h.status().result).toMatchObject({ page: { offset: 3, items: items.slice(3, 6) } })
    h.channel.neighbours({ ...query, edition }, target.key, 5)
    await expect.poll(() => h.status().result.phase, POLL).toBe("complete")
    expect(h.status().result).toMatchObject({ page: { total: 5, items: items.slice(3, 5) } })
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
    const catalog = createCatalog(snapshot)
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

it("a pending visible preview neither blocks list queries nor publishes after its channel closes", async () => {
  const h = harness()
  h.session.load(base, "jp", "preview")
  await h.settled()
  h.channel.search(query)
  await expect.poll(() => h.status().result.phase, POLL).toBe("complete")
  const first = h.status().result
  if (first.phase !== "complete") throw new Error("list pending")
  const summary = first.page.items[0]?.summary
  if (!summary) throw new Error("target missing")
  const detail = (built.manifest["files"] as JsonObject[]).find((file) =>
    (file["row_counts"] as JsonValue[][]).some(
      (count) =>
        count[0] === "face_revision" &&
        count[3] === "detail" &&
        count[2] === "set:" + (summary.cardNo.split("-")[0]?.toLowerCase() ?? "") &&
        file["region"] === "jp",
    ),
  )
  if (!detail) throw new Error("preview detail missing")
  const release = h.served.hold(stringValue(detail["path"]))
  const preview = h.session.createChannel()
  preview.preview(summary.printingId, summary.faceId, "zh-Hant", "jp")
  await expect.poll(() => h.served.requests.includes(stringValue(detail["path"])), POLL).toBe(true)
  h.channel.search({ ...query, state: { ...DEFAULT_QUERY, text: "BP01-051" } })
  await expect.poll(() => h.channel.getStatus().result.phase, POLL).toBe("complete")
  expect(preview.getStatus().result.phase).toBe("pending")
  preview.dispose()
  release()
  await h.queried()
  expect(
    h.events.filter((event) => event.kind === "preview-result" && event.channelId === preview.id),
  ).toHaveLength(0)
  h.session.dispose()
})

it("memory and persistent view hits do not rewrite the completed view", async () => {
  const storage = memoryCache()
  const served = origin()
  const h = harness(served, storage)
  h.session.load(base, "jp", "preview")
  await h.settled()
  const write = vi.spyOn(ViewCache.prototype, "write")
  try {
    const page = { ...query, offset: 1, limit: 1 }
    h.channel.view(page)
    const options = h.session.createChannel()
    options.view(page)
    await expect
      .poll(() => [h.status().result.phase, options.getStatus().result.phase], POLL)
      .toEqual(["complete", "complete"])
    expect(write).toHaveBeenCalledTimes(1)
    write.mockClear()
    h.channel.view(page, true)
    await expect.poll(() => h.status().result.phase, POLL).toBe("complete")
    await h.queried()
    expect(write).not.toHaveBeenCalled()
    const warm = harness(served, storage)
    warm.channel.view(page)
    warm.session.load(base, "jp", "preview")
    await warm.settled()
    await expect.poll(() => warm.status().result.phase, POLL).toBe("complete")
    expect(write).not.toHaveBeenCalled()
  } finally {
    write.mockRestore()
  }
})

describe("set releases and counts", () => {
  it("dates sets by their first linked release in the edition, also from a restored index", async () => {
    const storage = memoryCache()
    for (const edition of ["jp", "en", "jp"] as const) {
      const h = harness(origin(), storage)
      h.session.load(base, edition, "preview")
      await h.settled()
      h.channel.releases(edition)
      await expect.poll(() => h.status().result.phase, POLL).toBe("complete")
      const result = h.status().result
      if (result.phase !== "complete") throw new Error("missing releases")
      // The fixture dates every set but the promo one, and only the booster is a booster.
      const releasedOn = edition === "jp" ? "2026-02-01" : "2026-04-01"
      expect(result.releases?.find((set) => set.code === "bp01")).toMatchObject({
        publicCode: "BP01",
        kind: "booster",
        releasedOn,
      })
      expect(result.releases?.some((set) => set.code === "pr")).toBe(false)
      expect(result.releases?.every((set) => set.releasedOn === releasedOn)).toBe(true)
    }
  })

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
      expect(
        index.count({ state: { ...DEFAULT_QUERY, sets: ["bp01"] }, edition: "en" }, "zh-Hant"),
      ).toEqual({ cards: 15, translated: 14, proofreading: 0 })
      expect(
        index.count({ state: { ...DEFAULT_QUERY, sets: ["sd01"] }, edition: "en" }, "zh-Hant"),
      ).toEqual({ cards: 2, translated: 2, proofreading: 0 })
    }
  })

  it("counts each card of a set once with its name translations, and returns no cards", async () => {
    const served = origin()
    const baseline = createSnapshotClient(base, { fetch: served.fetcher })
    await baseline.load()
    const snapshot = baseline.snapshot()
    if (!snapshot) throw new Error("missing baseline")
    const catalog = createCatalog(snapshot)
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
    const expected = (language: "zh-Hant" | "en") => {
      const translated = cards.filter((name) => name.translations[language])
      return {
        cards: cards.length,
        translated: translated.length,
        proofreading: translated.filter(
          (name) => name.translationQuality?.[language]?.lowConfidence,
        ).length,
      }
    }
    // Synthetic names: every zh-Hant one is confident, English drafts without a counterpart are not.
    expect(expected("zh-Hant")).toMatchObject({ proofreading: 0 })
    expect(expected("zh-Hant").translated).toBeGreaterThan(0)
    expect(expected("en").proofreading).toBeGreaterThan(0)

    const h = harness(served)
    h.session.load(base, "jp", "preview")
    await h.settled()
    const state = { ...DEFAULT_QUERY, sets: ["bp01"] }
    for (const language of ["zh-Hant", "en"] as const) {
      h.channel.count({ state, edition: "jp" }, language)
      await expect.poll(() => h.status().result.phase, POLL).toBe("complete")
      const result = h.status().result
      if (result.phase !== "complete") throw new Error("missing counts")
      expect(result.counts).toEqual(expected(language))
      expect(result.page).toEqual({ total: cards.length, offset: 0, items: [] })
    }
    // Printings of one card in the set still count it once.
    h.channel.search({ state: { ...state, unit: "printing" }, edition: "jp", limit: 1 })
    await expect.poll(() => h.status().result.phase, POLL).toBe("complete")
    const listed = h.status().result
    if (listed.phase !== "complete") throw new Error("missing printings")
    expect(listed.page.total).toBeGreaterThan(cards.length)
    h.channel.count({ state: { ...state, unit: "printing" }, edition: "jp" }, "zh-Hant")
    await expect.poll(() => h.status().result.phase, POLL).toBe("complete")
    const printings = h.status().result
    if (printings.phase !== "complete") throw new Error("missing counts")
    expect(printings.counts?.cards).toBe(cards.length)
  })
})

it("effect URLs wait for the whole regional closure and return the same cold and warm answer", async () => {
  const served = origin()
  const h = harness(served)
  h.session.load(base, "jp", "preview")
  await h.settled()
  const file = (built.manifest["files"] as JsonObject[]).find(
    (file) => file["purpose"] === "detail" && file["region"] === "jp",
  )
  if (!file) throw new Error("detail fixture missing")
  const path = stringValue(file["path"])
  const release = served.hold(path)
  const advanced = { ...query, state: { ...DEFAULT_QUERY, text: "effect:defense" } }
  h.channel.search(advanced)
  await expect.poll(() => served.requests.includes(path), POLL).toBe(true)
  expect(h.status().result.phase).toBe("pending")
  release()
  await expect.poll(() => h.status().result.phase, POLL).not.toBe("pending")
  const cold = h.status().result
  if (cold.phase !== "complete")
    throw new Error(cold.phase === "error" ? cold.error.message : "effect search pending")
  expect(cold.page.total).toBeGreaterThan(0)
  expect(h.events.some((event) => event.kind === "query-progress")).toBe(true)
  h.channel.search(advanced)
  await expect.poll(() => h.status().result.phase, POLL).toBe("complete")
  const warm = h.status().result
  expect(warm.phase === "complete" && warm.page).toEqual(cold.page)
})

it("an unchanged-version check during the background download still completes printing facets", async () => {
  const h = harness()
  const print = (built.manifest["files"] as JsonObject[]).find(
    (file) => file["purpose"] === "print" && file["region"] === "jp",
  )
  if (!print) throw new Error("print fixture missing")
  const release = h.served.hold(stringValue(print["path"]))
  const readies = () => h.events.filter((event) => event.kind === "ready").length
  h.session.load(base, "jp", "preview")
  await expect.poll(readies, POLL).toBe(1)
  // A focus, online or retry check aborts the running background download.
  h.session.load(base, "jp", "preview")
  release()
  await expect.poll(readies, POLL).toBe(2)
  h.channel.search({ ...query, state: { ...DEFAULT_QUERY, rarities: ["legend"] } })
  await expect.poll(() => h.status().result.phase, POLL).toBe("complete")
  await h.settled()
}, 30_000)

it("a failed background printing closure is downloaded again by the next check", async () => {
  const h = harness()
  const print = (built.manifest["files"] as JsonObject[]).find(
    (file) => file["purpose"] === "print" && file["region"] === "jp",
  )
  if (!print) throw new Error("print fixture missing")
  const path = stringValue(print["path"])
  const bytes = h.served.files.get(path)
  if (!bytes) throw new Error("print bytes missing")
  h.served.files.delete(path)
  h.session.load(base, "jp", "preview")
  await h.settled()
  const rarity = { ...query, state: { ...DEFAULT_QUERY, rarities: ["legend"] } }
  h.channel.search(rarity)
  await expect.poll(() => h.status().result.phase, POLL).toBe("error")
  h.served.files.set(path, bytes)
  h.session.load(base, "jp", "preview")
  await h.settled()
  h.channel.search(rarity)
  await expect.poll(() => h.status().result.phase, POLL).toBe("complete")
}, 30_000)
