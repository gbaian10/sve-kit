// @vitest-environment node
import { describe, expect, it } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { DEFAULT_QUERY } from "../../domain/query/model"
import { apply } from "../../domain/search"
import { memoryCache } from "../../test-utils/cache"
import { createCatalog } from "../catalog"
import type { Fetcher } from "../cdn"
import { createSnapshotClient } from "../client"
import { canonical, type JsonObject, objectValue, stringValue } from "../format-v3/json"
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
type CombinedStatus = SearchStatus & SearchChannelStatus
import { SearchBlobs } from "./blobs"
import { BootstrapColumns } from "./bootstrap"
import { Columns, StringPool } from "./columns"
import { SearchEngine } from "./engine"
import { SearchIndex } from "./index"
import type { SearchEvent, SearchRequest } from "./messages"
import type { SearchTransportFactory } from "./session"

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
function harness(served = origin(), cacheStorage = memoryCache()) {
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
      { fetch: served.fetcher, cacheStorage },
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
  (file) => file["role"] === "bootstrap",
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

it("facets whose closure is not loaded report an error instead of a partial count", async () => {
  const h = harness()
  h.session.load(base, "jp", "preview")
  await h.settled()
  h.channel.search({ ...query, state: { ...DEFAULT_QUERY, rarities: ["legend"] } })
  await h.queried()
  expect(h.status().result).toEqual({
    phase: "error",
    error: {
      kind: "unsupported-query",
      message: "requested search facet is not ready",
      facets: ["rarities"],
    },
  })
})

it("set filters follow the printing owner dictionary and open a matching printing", async () => {
  const served = origin()
  const client = createSnapshotClient(base, { fetch: served.fetcher })
  await client.load()
  const snapshot = client.snapshot()
  if (!snapshot) throw new Error("missing fixture")
  const columns = new BootstrapColumns()
  for (const fragment of snapshot.bootstrap) {
    if (fragment.table !== "printing") {
      columns.ingest([fragment])
      continue
    }
    for (const row of fragment.rows)
      columns.ingest([
        {
          ...fragment,
          rows: [row],
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
    [["types"], { types: ["follower"] }],
    [["rarities"], { rarities: ["legend"] }],
    [["altArtOnly"], { altArtOnly: true }],
    [["mechanics"], { mechanics: { quick: "has" } }],
    [["unit"], { unit: "printing" }],
    [["sort"], { sort: "cost" }],
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

it("cost columns preserve the signed safe integers accepted by the snapshot reader", async () => {
  const client = createSnapshotClient(base, { fetch: origin().fetcher })
  await client.load()
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
