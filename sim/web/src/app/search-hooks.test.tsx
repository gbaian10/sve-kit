import {
  act,
  render as testingRender,
  renderHook as testingRenderHook,
  screen,
  waitFor,
} from "@testing-library/react"
import { type ReactNode, useState } from "react"
import { MemoryRouter } from "react-router"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import { ClassQuickBar } from "../components/search/ClassQuickBar"
import type { SearchQuery } from "../data"
import type { SearchEvent, SearchRequest } from "../data/search/messages"
import { DEFAULT_QUERY } from "../domain/query/model"
import { prefsStore } from "../settings"
import {
  useAbilityPreview,
  useListView,
  usePrintingOptions,
  useSearch,
  useSearchReadiness,
  useSetOptions,
} from "./snapshot"

function Router({ children }: { children: ReactNode }) {
  return <MemoryRouter>{children}</MemoryRouter>
}
const renderHook: typeof testingRenderHook = (hook, options) =>
  testingRenderHook(hook, { wrapper: Router, ...options })
function render(element: ReactNode) {
  return testingRender(element, { wrapper: Router })
}

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
function answerViews(
  classes: NonNullable<ReturnType<typeof useSetOptions>["classes"]> = [],
  ids = ["p"],
) {
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
          items: ids.map((printingId, i) => ({
            key: `c:${String(offset + i)}`,
            printingId,
            summary: undefined,
          })),
        },
        options: {
          keywords: [],
          keywordCoverage: "unknown",
          types: [],
          typeCounts: {},
          locationCounts: {},
          traits: [],
          traitCounts: {},
          artists: [],
          artistCounts: {},
          classes,
          sets: [],
          quickSets: {},
          rarities: [],
          prBadgeStyles: [],
          rarityCounts: {},
          setCounts: {},
        },
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
  prefsStore.set({ cardEdition: "jp" })
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
    expect(result.current.loading).toBe(true)
    expect(result.current.view).toBe(shown)
    expect(fake.requests.filter((request) => request.kind === "view").slice(before)).toMatchObject([
      { query: { offset: 60, limit: 60 } },
    ])
    answerViews()
    expect(result.current.loading).toBe(false)
    expect(result.current.view?.page.items.map((item) => item.key)).toEqual(["c:0", "c:60"])
    // Returning to fewer pages shows only those, without asking the Worker again.
    const asked = fake.requests.length
    rerender({ pages: 1 })
    expect(result.current.view?.page.items.map((item) => item.key)).toEqual(["c:0"])
    expect(fake.requests).toHaveLength(asked)
    rerender({ pages: 2 })
    expect(result.current.view?.page.items.map((item) => item.key)).toEqual(["c:0", "c:60"])
    expect(fake.requests).toHaveLength(asked)
  })
  it("retains loaded rows on an appended-page failure and retries only the failed page", () => {
    const { result, rerender } = renderHook(
      ({ pages }: { pages: number }) => useListView(query, pages),
      { initialProps: { pages: 1 } },
    )
    ready()
    answerViews()
    const shown = result.current.view
    rerender({ pages: 2 })
    const request = fake.requests.findLast(
      (request) => request.kind === "view" && request.query.offset === 60,
    )
    if (request?.kind !== "view") throw new Error("missing appended-page request")
    emit({
      kind: "query-error",
      generation: request.generation,
      channelId: request.channelId,
      queryId: request.queryId,
      error: { kind: "query-failed", message: "synthetic network error" },
    })
    expect(result.current.view).toBe(shown)
    expect(result.current.loading).toBe(false)
    expect(result.current.error?.kind).toBe("query-failed")
    const before = fake.requests.length
    act(() => {
      result.current.retry()
    })
    expect(fake.requests.slice(before)).toMatchObject([{ kind: "view", query: { offset: 60 } }])
    expect(result.current.view).toBe(shown)
    expect(result.current.loading).toBe(true)
    expect(result.current.error).toBeUndefined()
    answerViews()
    expect(result.current.view?.page.items.map((item) => item.key)).toEqual(["c:0", "c:60"])
    expect(result.current.loading).toBe(false)
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

it("the class quickbar keeps its cells during a same-edition version replacement", () => {
  function QuickBar() {
    const { classes } = useSetOptions()
    return (
      <ClassQuickBar
        label="classes"
        selected={[]}
        onToggle={() => undefined}
        classes={[
          { code: "neutral", label: "neutral" },
          ...(classes ?? []).map((item) => ({ code: item.code, label: item.name.original.text })),
        ]}
      />
    )
  }
  render(<QuickBar />)
  ready()
  const classes = [
    "forestcraft",
    "swordcraft",
    "runecraft",
    "dragoncraft",
    "abysscraft",
    "havencraft",
  ].map((code) => ({
    id: code,
    code,
    name: { original: { lang: "ja" as const, text: code }, translations: {} },
  }))
  answerViews(classes)
  const cells = () =>
    screen.getByRole("group", { name: "classes" }).querySelectorAll("button[aria-pressed]")
  expect(cells()).toHaveLength(7)
  act(() => {
    window.dispatchEvent(new Event("online"))
  })
  ready("v2")
  expect(cells()).toHaveLength(7)
  answerViews(classes.slice(0, 5))
  expect(cells()).toHaveLength(6)
  act(() => {
    prefsStore.set({ cardEdition: "en" })
  })
  expect(cells()).toHaveLength(1)
})

it("visible ability previews stay complete during replacement but never cross editions", () => {
  const { result, rerender } = renderHook(
    ({ edition }: { edition: "jp" | "en" }) => useAbilityPreview("p", "f", "zh-Hant", edition),
    { initialProps: { edition: "jp" } },
  )
  const answerPreview = (text: string) => {
    const request = fake.requests.findLast((request) => request.kind === "preview")
    if (request?.kind !== "preview") throw new Error("preview request missing")
    emit({
      kind: "preview-result",
      generation: request.generation,
      channelId: request.channelId,
      queryId: request.queryId,
      preview: { text, language: "zh-Hant" },
    })
  }
  ready()
  answerPreview("old synthetic ability")
  act(() => {
    window.dispatchEvent(new Event("online"))
  })
  ready("v2")
  expect(result.current).toMatchObject({
    phase: "complete",
    preview: { text: "old synthetic ability" },
  })
  answerPreview("new synthetic ability")
  expect(result.current).toMatchObject({
    phase: "complete",
    preview: { text: "new synthetic ability" },
  })
  rerender({ edition: "en" })
  expect(result.current).toMatchObject({ phase: "error", error: { kind: "edition-not-ready" } })
})

it("prefetches nearby result slots only and retains faces until the snapshot changes", async () => {
  const observers: {
    callback: IntersectionObserverCallback
    nodes: Set<Element>
    options: IntersectionObserverInit | undefined
  }[] = []
  vi.stubGlobal(
    "IntersectionObserver",
    class {
      entry: (typeof observers)[number]
      constructor(callback: IntersectionObserverCallback, options?: IntersectionObserverInit) {
        this.entry = { callback, nodes: new Set(), options }
        observers.push(this.entry)
      }
      observe(node: Element) {
        this.entry.nodes.add(node)
      }
      unobserve(node: Element) {
        this.entry.nodes.delete(node)
      }
      disconnect() {
        this.entry.nodes.clear()
      }
    },
  )
  function Results() {
    const [filtered, setFiltered] = useState(false)
    const { view, images, imageRoot } = useListView(
      { ...query, state: { ...DEFAULT_QUERY, text: filtered ? "synthetic" : "" } },
      1,
    )
    return (
      <>
        <button
          onClick={() => {
            setFiltered(true)
          }}
        >
          Filter
        </button>
        <span data-testid="outside" data-image-printing="outside" />
        <div ref={imageRoot}>
          {view?.page.items.map((item) => (
            <span
              key={item.printingId}
              data-testid={item.printingId}
              data-image-printing={item.printingId}
            />
          ))}
          <output data-testid="known">
            {["p", "q"].filter((id) => images?.known?.(id, "f")).join(",")}
          </output>
        </div>
      </>
    )
  }
  render(<Results />)
  ready()
  answerViews([], ["p", "q"])
  expect(screen.getByTestId("known")).toBeEmptyDOMElement()
  const near = (id: string, distance: number) => {
    const node = screen.getByTestId(id)
    const observer = observers.findLast((observer) => observer.nodes.has(node))
    if (!observer) throw new Error("slot is not observed")
    const margin = Number.parseFloat(observer.options?.rootMargin ?? "0")
    act(() => {
      observer.callback(
        [
          {
            target: node,
            isIntersecting: distance <= margin,
            boundingClientRect: node.getBoundingClientRect(),
            intersectionRatio: 1,
            intersectionRect: node.getBoundingClientRect(),
            rootBounds: null,
            time: 0,
          },
        ],
        {} as IntersectionObserver,
      )
    })
  }
  const imagesRequests = () => fake.requests.filter((request) => request.kind === "images")
  near("p", window.innerHeight)
  await waitFor(() => {
    expect(imagesRequests().at(-1)).toMatchObject({ printingIds: ["p"] })
  })
  expect(observers.some((observer) => observer.nodes.has(screen.getByTestId("outside")))).toBe(
    false,
  )
  const answerImages = (printingId: string) => {
    const request = imagesRequests().at(-1)
    if (!request) throw new Error("image request missing")
    emit({
      kind: "images-result",
      generation: request.generation,
      channelId: request.channelId,
      queryId: request.queryId,
      faces: [
        { printingId, faceId: "f", intId: 1, ordinal: 0, asset: { availability: "missing" } },
      ],
    })
  }
  answerImages("p")
  expect(screen.getByTestId("known")).toHaveTextContent("p")
  const count = imagesRequests().length
  near("p", window.innerHeight * 3)
  near("p", 0)
  expect(imagesRequests()).toHaveLength(count)
  near("p", window.innerHeight * 3)
  near("q", 0)
  await waitFor(() => {
    expect(imagesRequests().at(-1)).toMatchObject({ printingIds: ["q"] })
  })
  answerImages("q")
  expect(screen.getByTestId("known")).toHaveTextContent("p,q")
  act(() => {
    screen.getByRole("button", { name: "Filter" }).click()
  })
  answerViews([], ["p", "q"])
  expect(screen.getByTestId("known")).toHaveTextContent("p,q")
  const beforeReturn = imagesRequests().length
  near("p", 0)
  expect(imagesRequests()).toHaveLength(beforeReturn)
  act(() => {
    window.dispatchEvent(new Event("online"))
  })
  ready("v2")
  expect(screen.getByTestId("known")).toBeEmptyDOMElement()
})

it("keeps rarity and type lists pending independently of the basic set and class view", () => {
  const { result } = renderHook(() => ({
    classes: useSetOptions().classes,
    options: usePrintingOptions(),
  }))
  ready()
  const requests = fake.requests.filter((request) => request.kind === "view")
  const basic = requests.findLast((request) => request.query.state.rarities.length === 0)
  const rarity = requests.findLast((request) => request.query.state.rarities.includes("pr"))
  if (!basic || !rarity) throw new Error("missing option requests")
  const keywords = [
    {
      id: "kw:synthetic",
      code: "synthetic",
      name: { original: { lang: "ja" as const, text: "合成" }, translations: {} },
    },
  ]
  const answer = (
    request: typeof basic,
    prBadgeStyles: readonly string[],
    keywordCoverage: "unknown" | "pending" = "unknown",
  ) => {
    emit({
      kind: "view-result",
      generation: request.generation,
      channelId: request.channelId,
      queryId: request.queryId,
      view: {
        query: request.query,
        page: { total: 0, offset: 0, items: [] },
        options: {
          keywords,
          keywordCoverage,
          types: [],
          typeCounts: { leader: 2 },
          locationCounts: { leader: 2, "leader:gold": 2 },
          traits: [],
          traitCounts: {},
          artists: [],
          artistCounts: {},
          classes: [],
          sets: [],
          quickSets: {},
          rarities: [],
          prBadgeStyles,
          rarityCounts: { pr: 2 },
          setCounts: {},
        },
        faces: [],
      },
    })
  }
  answer(basic, [])
  expect(result.current.classes).toEqual([])
  expect(result.current.options).toBeUndefined()
  answer(rarity, ["br", "unknown"], "pending")
  expect(result.current.options?.rarity.styles).toEqual(["br", "unknown"])
  expect(result.current.options?.keyword).toEqual({ keywords, coverage: "pending" })
  answer(rarity, ["br", "unknown"])
  expect(result.current.options).toEqual({
    abilityTypes: { coverage: "pending" },
    rarity: { styles: ["br", "unknown"], counts: { pr: 2 } },
    type: { types: [], typeCounts: { leader: 2 } },
    location: { types: [], counts: { leader: 2, "leader:gold": 2 } },
    trait: { traits: [], counts: {} },
    artist: { artists: [], counts: {} },
    keyword: { keywords, coverage: "unknown" },
  })
})
