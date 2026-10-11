import { act, renderHook } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import type { SearchEvent, SearchQuery, SearchRequest } from "../data"
import { DEFAULT_QUERY } from "../domain/query/model"
import { useListView, useSearch, useSearchReadiness } from "./snapshot"

// Control the browser boundary; the real session still owns generations and query identities.
const fake = vi.hoisted(() => ({
  requests: [] as SearchRequest[],
  client: { base: "/cdn", snapshot: () => null },
}))
vi.mock("../data", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../data")>()),
  snapshotClient: () => fake.client,
  useSnapshotRoot: () => "preview",
  activeSnapshotRoot: () => "preview",
}))
const hookWorkers: HookWorker[] = []
class HookWorker {
  onmessage: ((event: MessageEvent<SearchEvent>) => void) | null = null
  onerror: (() => void) | null = null
  onmessageerror: (() => void) | null = null
  constructor() {
    hookWorkers.push(this)
  }
  postMessage(request: SearchRequest) {
    fake.requests.push(request)
  }
  terminate() {}
}
function emit(event: SearchEvent) {
  act(() => {
    hookWorkers.at(-1)?.onmessage?.({ data: event } as MessageEvent<SearchEvent>)
  })
}
function load() {
  const request = fake.requests.findLast((request) => request.kind === "load")
  if (request?.kind !== "load") throw new Error("load request missing")
  return request
}
const metrics = {
  restored: true,
  files: 0,
  maxRawBytes: 0,
  parseMs: 0,
  buildMs: 0,
  wallMs: 0,
  firstFileMs: 0,
  firstSetMs: 0,
  arrayBuffers: 0,
  stringBytes: 0,
  cards: 2,
  stagingArrayBuffers: 0,
  stagingStringBytes: 0,
}
function ready(version = "v1") {
  emit({
    kind: "ready",
    generation: load().generation,
    edition: "jp",
    root: "/cdn",
    dataVersion: version,
    manifestHash: `sha256:${version}`,
    metrics,
  })
}
function answer() {
  const request = fake.requests.findLast((request) => request.kind === "search")
  if (request?.kind !== "search") throw new Error("search request missing")
  emit({
    kind: "result",
    generation: request.generation,
    channelId: request.channelId,
    queryId: request.queryId,
    page: { total: 2, offset: request.query.offset ?? 0, items: [] },
  })
}
/** Answers each channel's latest view request with one item named after its page offset. */
function answerViews() {
  const latest = new Map<number, Extract<SearchRequest, { kind: "view" }>>()
  for (const request of fake.requests)
    if (request.kind === "view") latest.set(request.channelId, request)
  for (const request of latest.values()) {
    const offset = request.query.offset ?? 0
    emit({
      kind: "view-result",
      generation: request.generation,
      channelId: request.channelId,
      queryId: request.queryId,
      view: {
        query: request.query,
        page: {
          total: 120,
          offset,
          items: [{ key: `c:${String(offset)}`, printingId: "p", summary: undefined }],
        },
        options: { classes: [], sets: [] },
        faces: [],
      },
    })
  }
}
const query: SearchQuery = { state: DEFAULT_QUERY, edition: "jp" }
beforeEach(() => {
  act(() => {
    hookWorkers.at(-1)?.onerror?.()
  })
  fake.requests.length = 0
  vi.stubGlobal("Worker", HookWorker)
})
afterEach(() => vi.unstubAllGlobals())

describe("Worker hooks", () => {
  it("maps root names explicitly and exposes Worker progress and generation", () => {
    const { result } = renderHook(() => useSearchReadiness("jp"))
    expect(result.current).toMatchObject({
      phase: "downloading",
      generation: null,
      loading: { root: "preview", edition: "jp", step: "index" },
    })
    emit({ kind: "stage", generation: load().generation, step: "bootstrap" })
    emit({ kind: "progress", generation: load().generation, done: 2, total: 84, persistent: true })
    expect(result.current.loading).toMatchObject({ step: "bootstrap", done: 2, total: 84 })
    ready()
    expect(result.current).toMatchObject({
      phase: "ready",
      root: "preview",
      edition: "jp",
      dataVersion: "v1",
      generation: load().generation,
    })
    expect(fake.client.snapshot()).toBeNull()
  })
  it("keeps the active generation during a replacement load", () => {
    const { result } = renderHook(() => useSearchReadiness("jp"))
    ready()
    const first = result.current.generation
    act(() => {
      window.dispatchEvent(new Event("online"))
    })
    emit({ kind: "stage", generation: load().generation, step: "bootstrap" })
    expect(result.current).toMatchObject({
      phase: "ready",
      generation: first,
      updating: true,
      loading: { root: "preview", step: "bootstrap" },
    })
    ready("v2")
    expect(result.current.generation).toBe(load().generation)
    expect(result.current.generation).not.toBe(first)
  })
  it("reports a failed load and retries through the Worker", () => {
    const { result } = renderHook(() => useSearchReadiness("jp"))
    emit({ kind: "error", generation: load().generation, message: "offline" })
    expect(result.current).toMatchObject({
      phase: "error",
      error: { kind: "load-failed", message: "offline" },
    })
    const first = load().generation
    act(() => {
      result.current.retry()
    })
    expect(load().generation).toBeGreaterThan(first)
    expect(result.current.phase).toBe("downloading")
  })
  it("remembers pending input, then accepts only that channel's response", () => {
    const { result, rerender } = renderHook<
      ReturnType<typeof useSearch>,
      { input: SearchQuery | null }
    >(({ input }: { input: SearchQuery | null }) => useSearch(input), {
      initialProps: { input: query },
    })
    expect(result.current).toEqual({ lastInput: query, result: { phase: "pending" } })
    rerender({ input: null })
    expect(result.current.lastInput).toEqual(query)
    ready()
    answer()
    expect(result.current.result).toMatchObject({
      phase: "complete",
      root: "/cdn",
      page: { total: 2, offset: 0 },
    })
  })
  it("passes bounded paging to the Worker", () => {
    const { result } = renderHook(() => useSearch({ ...query, offset: 1, limit: 1 }))
    ready()
    const request = fake.requests.findLast((request) => request.kind === "search")
    expect(request).toMatchObject({ query: { offset: 1, limit: 1 } })
    answer()
    expect(result.current.result).toMatchObject({
      phase: "complete",
      page: { total: 2, offset: 1 },
    })
  })
  it("keeps typed query failures explicit", () => {
    const { result } = renderHook(() =>
      useSearch({ ...query, state: { ...DEFAULT_QUERY, sort: "cost" } }),
    )
    ready()
    const request = fake.requests.findLast((request) => request.kind === "search")
    if (request?.kind !== "search") throw new Error("request missing")
    emit({
      kind: "query-error",
      generation: request.generation,
      channelId: request.channelId,
      queryId: request.queryId,
      error: {
        kind: "unsupported-query",
        message: "requested search facet is not ready",
        facets: ["sort"],
      },
    })
    expect(result.current.result).toMatchObject({
      phase: "error",
      error: { kind: "unsupported-query", facets: ["sort"] },
    })
  })
  it("a failed load cannot become an empty result", () => {
    const { result } = renderHook(() => useSearch(query))
    emit({ kind: "error", generation: load().generation, message: "offline" })
    expect(result.current.result).toMatchObject({
      phase: "error",
      error: { kind: "load-unavailable", message: "offline" },
    })
  })
  it("loading more keeps the shown page on screen and asks only for the new page", () => {
    const { result, rerender } = renderHook(
      ({ pages }: { pages: number }) => useListView(query, pages),
      { initialProps: { pages: 1 } },
    )
    ready()
    answerViews()
    const shown = result.current.view
    expect(shown?.page.items.map((item) => item.key)).toEqual(["c:0"])
    const before = fake.requests.filter((request) => request.kind === "view").length
    rerender({ pages: 2 })
    expect(result.current.view).toBe(shown)
    expect(fake.requests.filter((request) => request.kind === "view").slice(before)).toMatchObject([
      { query: { offset: 60, limit: 60 } },
    ])
    answerViews()
    expect(result.current.view?.page.items.map((item) => item.key)).toEqual(["c:0", "c:60"])
    // Returning to fewer pages shows only those, without asking the Worker again.
    const asked = fake.requests.length
    rerender({ pages: 1 })
    expect(result.current.view?.page.items.map((item) => item.key)).toEqual(["c:0"])
    expect(fake.requests).toHaveLength(asked)
  })
  it("a newer data version keeps the shown pages until its own pages arrive", () => {
    const { result } = renderHook(() => useListView(query, 1))
    ready()
    answerViews()
    const shown = result.current.view
    act(() => {
      window.dispatchEvent(new Event("online"))
    })
    ready("v2")
    expect(result.current.view).toBe(shown)
    answerViews()
    expect(result.current.view).toBeDefined()
    expect(result.current.view).not.toBe(shown)
  })
  it("has no result without input", () => {
    const { result } = renderHook(() => useSearch(null))
    ready()
    expect(result.current).toEqual({ lastInput: null, result: { phase: "pending" } })
  })
})
