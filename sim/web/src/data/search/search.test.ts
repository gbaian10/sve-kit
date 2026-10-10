// @vitest-environment node
import { describe, expect, it } from "vitest"

import { buildSnapshot } from "../../../scripts/fixture/build"
import { DEFAULT_QUERY } from "../../domain/query/model"
import { memoryCache } from "../../test-utils/cache"
import { catalogFromIndex, createCatalog } from "../catalog"
import type { Fetcher } from "../cdn"
import { createSnapshotClient } from "../client"
import { canonical, type JsonObject, objectValue, stringValue } from "../format-v3/json"
import { digest } from "../format-v3/sha256"
import { type SearchPage, type SearchQuery, SearchSession, type SearchStatus } from "../index"
import { BootstrapColumns } from "./bootstrap"
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
  return {
    session,
    events,
    requests,
    served,
    crash: () => {
      crash()
    },
    settled: () => pending,
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
  return {
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
    const observed: SearchStatus[] = []
    h.session.subscribe(() => {
      observed.push(h.session.getStatus())
    })
    const release = h.served.hold(heldPath)
    h.session.load(base, "jp", "preview")
    for (const text of ["first", "second", "bp01-001"])
      h.session.search({ ...query, state: { ...DEFAULT_QUERY, text } })
    expect(h.session.getStatus()).toMatchObject({
      phase: "downloading",
      generation: null,
      result: { phase: "pending" },
    })
    expect(h.requests.filter((request) => request.kind === "search")).toHaveLength(0)
    await expect.poll(() => h.served.requests.includes(heldPath), POLL).toBe(true)
    expect(h.session.getStatus().result.phase).toBe("pending")
    expect(h.events.some((event) => event.kind === "ready")).toBe(false)
    release()
    await h.settled()
    expect(h.requests.filter((request) => request.kind === "search")).toHaveLength(1)
    expect(h.session.getStatus().lastInput?.state.text).toBe("bp01-001")
    const result = h.session.getStatus().result
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
    h.session.search(query)
    const first = h.session.getStatus().result
    const blobs = h.served.requests.filter((path) => path.startsWith("snapshots/blobs/"))
    h.served.update(2)
    const release = h.served.hold("snapshots/preview/current.json")
    h.session.load(base, "jp", "preview")
    h.session.search(query)
    expect(h.session.getStatus()).toMatchObject({
      phase: "ready",
      generation: 1,
      updating: true,
      result: { phase: "complete" },
    })
    release()
    await h.settled()
    expect(h.session.getStatus()).toMatchObject({
      generation: 2,
      updating: false,
      result: { phase: "complete" },
    })
    expect(h.served.requests.filter((path) => path.startsWith("snapshots/blobs/"))).toEqual(blobs)
    const result = h.session.getStatus().result
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
    h.session.search(query)
    expect(h.session.getStatus()).toMatchObject({
      phase: "ready",
      generation: 1,
      updating: false,
      result: { phase: "complete" },
    })
    expect(h.session.getStatus().error).toContain("HTTP 404")
  })

  it("cancel/restart never publishes partial data, and retry rebuilds from cache", async () => {
    const h = harness()
    const release = h.served.hold(heldPath)
    h.session.load(base, "jp", "preview")
    h.session.search(query)
    h.session.cancel()
    release()
    await h.settled()
    expect(h.events.some((event) => event.kind === "ready")).toBe(false)
    h.session.retry()
    await h.settled()
    expect(h.session.getStatus().result.phase).toBe("complete")
    const count = h.served.requests.filter((path) => path.startsWith("snapshots/blobs/")).length
    h.crash()
    expect(h.session.getStatus()).toMatchObject({ phase: "error", generation: null })
    h.session.retry()
    await h.settled()
    expect(h.session.getStatus().result.phase).toBe("complete")
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
        h.session.search({ ...query, state, limit: 2 })
        const result = h.session.getStatus().result
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
      h.session.search({ ...query, state: { ...DEFAULT_QUERY, cost } })
      const result = h.session.getStatus().result
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
    h.session.search({ ...query, edition: "en" })
    await h.settled()
    expect(h.session.getStatus().result.phase).toBe("complete")
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
  const emptyPage: SearchPage = { total: 0, offset: 0, items: [] }
  const ready = (generation: number): SearchEvent => ({
    kind: "ready",
    generation,
    edition: "jp",
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
    },
  })
  session.load(base, "jp")
  session.search(query)
  receive(ready(1))
  const previous = sent.findLast((request) => request.kind === "search")
  if (!previous) throw new Error("missing query")
  session.search({ ...query, state: { ...DEFAULT_QUERY, text: "last" } })
  receive({
    kind: "result",
    generation: 1,
    queryId: previous.queryId,
    page: { total: 1000, offset: 0, items: [] },
  })
  expect(session.getStatus().result.phase).toBe("pending")
  session.load(base, "jp")
  receive(ready(2))
  const current = sent.findLast((request) => request.kind === "search")
  if (!current) throw new Error("missing query")
  receive({
    kind: "result",
    generation: 1,
    queryId: current.queryId,
    page: { total: 1000, offset: 0, items: [] },
  })
  expect(session.getStatus().result.phase).toBe("pending")
  receive({
    kind: "result",
    generation: 2,
    queryId: current.queryId,
    page: emptyPage,
  })
  expect(session.getStatus().result).toMatchObject({ phase: "complete", page: { total: 0 } })
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
  h.session.search(query)
  await h.worker()
  h.main()
  expect(h.session.getStatus()).toMatchObject({
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
  h.session.search(query)
  await h.worker()
  h.main()
  expect(h.session.getStatus()).toMatchObject({
    phase: "ready",
    generation: 2,
    updating: false,
    result: { phase: "complete", generation: 2 },
  })
  expect(h.session.getStatus().error).toContain("HTTP 404")
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
  session.load(base, "jp")
  session.search(query)
  failures[0]?.()
  expect(session.getStatus().result.phase).toBe("error")
  session.load(base, "en")
  expect(workers).toHaveLength(2)
  expect(workers[1]).toMatchObject([{ kind: "load", edition: "en" }])
  expect(session.getStatus()).toMatchObject({ phase: "downloading", result: { phase: "pending" } })
  failures[0]?.()
  expect(session.getStatus().phase).toBe("downloading")
})

it("switching editions waits for the new region instead of returning old-region hits", async () => {
  const h = harness()
  h.session.load(base, "jp", "preview")
  await h.settled()
  const release = h.served.hold("snapshots/preview/current.json")
  h.session.load(base, "en", "preview")
  h.session.search({ ...query, edition: "en" })
  expect(h.session.getStatus().result.phase).toBe("pending")
  release()
  await h.settled()
  const result = h.session.getStatus().result
  if (result.phase !== "complete") throw new Error("missing result")
  expect(result.page.items.every((item) => item.summary?.region === "en")).toBe(true)
})

it("a replaced staging load drains without publishing its generation", async () => {
  const h = harness()
  const release = h.served.hold(heldPath)
  h.session.load(base, "jp", "preview")
  await expect.poll(() => h.served.requests.includes(heldPath), POLL).toBe(true)
  h.session.load(base, "jp", "preview")
  h.session.search(query)
  release()
  await h.settled()
  expect(
    h.events.filter((event) => event.kind === "ready").map((event) => event.generation),
  ).toEqual([2])
  expect(h.session.getStatus().generation).toBe(2)
})

it("torn bytes cannot become active, and typing after failure keeps the error explicit", async () => {
  const h = harness()
  h.served.files.set(heldPath, new Uint8Array([1, 2, 3]))
  h.session.load(base, "jp", "preview")
  await h.settled()
  h.session.search(query)
  expect(h.session.getStatus()).toMatchObject({
    phase: "error",
    generation: null,
    result: { phase: "error" },
  })
  expect(h.events.some((event) => event.kind === "ready")).toBe(false)
  expect(h.served.requests.filter((path) => path === heldPath)).toHaveLength(2)
  h.served.files.set(heldPath, built.files.get(heldPath) ?? new Uint8Array())
  h.session.retry()
  await h.settled()
  expect(h.session.getStatus().result.phase).toBe("complete")
})

it("facets whose closure is not loaded report an error instead of a partial count", async () => {
  const h = harness()
  h.session.load(base, "jp", "preview")
  await h.settled()
  h.session.search({ ...query, state: { ...DEFAULT_QUERY, rarities: ["legend"] } })
  expect(h.session.getStatus().result).toEqual({
    phase: "error",
    message: "requested search facet is not ready",
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
  const index = new SearchIndex(
    catalogFromIndex(columns.index(), columns.rows, snapshot.config),
    "jp",
  )
  const page = index.search({ ...query, state: { ...DEFAULT_QUERY, sets: ["sd01"] } })
  expect(page.total).toBe(1)
  expect(page.items[0]?.printingId).toBe("p:bp01-001")
})
