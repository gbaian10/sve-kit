import { act, renderHook } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import type { LoadedSnapshot, SearchQuery, SnapshotStatus } from "../data"
import { DEFAULT_QUERY } from "../domain/query/model"
import { useSearch, useSearchReadiness } from "./snapshot"

// A controllable data boundary isolates the hooks' readiness and query behaviour.
const fake = vi.hoisted(() => {
  const listeners = new Set<() => void>()
  const state = {
    status: { state: "idle" } as SnapshotStatus,
    snapshot: null as LoadedSnapshot | null,
    retried: 0,
    reloaded: 0,
  }
  const notify = () => {
    for (const listener of listeners) listener()
  }
  const client = {
    base: "/cdn",
    status: () => state.status,
    subscribe: (listener: () => void) => {
      listeners.add(listener)
      return () => {
        listeners.delete(listener)
      }
    },
    load: () => Promise.resolve(),
    retry: () => {
      state.retried += 1
      return Promise.resolve()
    },
    reload: () => {
      state.reloaded += 1
      return Promise.resolve()
    },
    snapshot: () => state.snapshot,
    metadataStatus: () => ({ state: "idle" as const, done: 0, total: 0, persistent: false }),
    prefetchImages: () => Promise.resolve(),
    cancelImagePrefetch: () => undefined,
    fragments: () => Promise.reject(new Error("no")),
  }
  const set = (status: SnapshotStatus, snapshot: LoadedSnapshot | null) => {
    state.status = status
    state.snapshot = snapshot
    notify()
  }
  return { state, client, set }
})

vi.mock("../data", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../data")>()),
  snapshotClient: () => fake.client,
  useSnapshotRoot: () => "preview",
  catalogOf: () => ({
    results: (state: { readonly text: string }) =>
      [
        { key: "c:one", printingId: "p:one" },
        { key: "c:two", printingId: "p:two" },
      ].filter((hit) => state.text === "" || hit.key.includes(state.text)),
    summary: (printingId: string) => ({ printingId, cardNo: printingId.toUpperCase() }),
  }),
}))

const loaded = (dataVersion: string): LoadedSnapshot =>
  ({ dataVersion, manifestHash: `sha256:${dataVersion}` }) as unknown as LoadedSnapshot

const query: SearchQuery = { state: DEFAULT_QUERY, edition: "jp" }

beforeEach(() => {
  fake.set({ state: "idle" }, null)
  fake.state.retried = 0
  fake.state.reloaded = 0
})

afterEach(() => {
  vi.clearAllMocks()
})

describe("useSearchReadiness", () => {
  it("reports the download step, then the generation with its root, edition and version", () => {
    fake.set({ state: "loading", phase: "manifest" }, null)
    const { result } = renderHook(() => useSearchReadiness("jp"))
    expect(result.current).toMatchObject({
      phase: "downloading",
      generation: null,
      loading: { root: "preview", edition: "jp", step: "manifest" },
      updating: false,
    })
    act(() => {
      fake.set({ state: "ready", dataVersion: "v1" }, loaded("v1"))
    })
    expect(result.current).toMatchObject({
      phase: "ready",
      root: "preview",
      edition: "jp",
      dataVersion: "v1",
      manifestHash: "sha256:v1",
      updating: false,
    })
    expect(result.current.generation).toBeTypeOf("number")
  })

  it("keeps the old generation queryable while an update downloads, and bumps it on a new snapshot", () => {
    const v1 = loaded("v1")
    fake.set({ state: "ready", dataVersion: "v1" }, v1)
    const { result } = renderHook(() => useSearchReadiness("jp"))
    const first = result.current.generation
    act(() => {
      fake.set({ state: "ready", dataVersion: "v1", updating: "bootstrap" }, v1)
    })
    expect(result.current).toMatchObject({
      phase: "ready",
      updating: true,
      loading: { step: "bootstrap" },
    })
    expect(result.current.generation).toBe(first)
    act(() => {
      fake.set({ state: "ready", dataVersion: "v2" }, loaded("v2"))
    })
    expect(result.current.generation).not.toBe(first)
  })

  it("exposes a failed load with a retry that reaches the client", () => {
    fake.set({ state: "error", kind: "network", detail: "offline" }, null)
    const { result } = renderHook(() => useSearchReadiness("jp"))
    expect(result.current).toMatchObject({
      phase: "error",
      error: { kind: "load-failed", message: "network: offline" },
    })
    act(() => {
      result.current.retry()
    })
    expect(fake.state.retried).toBe(1)
    act(() => {
      fake.set(
        { state: "ready", dataVersion: "v1", updateError: { kind: "corrupt", detail: "x" } },
        loaded("v1"),
      )
    })
    expect(result.current).toMatchObject({ phase: "ready", error: { kind: "load-failed" } })
    act(() => {
      result.current.retry()
    })
    expect(fake.state.reloaded).toBe(1)
  })
})

describe("useSearch", () => {
  it("answers pending while downloading, keeps the input, and answers it once ready", () => {
    fake.set({ state: "loading", phase: "bootstrap" }, null)
    const initial: { readonly input: SearchQuery | null } = { input: query }
    const { result, rerender } = renderHook((props) => useSearch(props.input), {
      initialProps: initial,
    })
    expect(result.current).toEqual({ lastInput: query, result: { phase: "pending" } })
    // The caller stops passing the query (e.g. a re-render without it): the input is remembered.
    rerender({ input: null })
    expect(result.current.lastInput).toEqual(query)
    act(() => {
      fake.set({ state: "ready", dataVersion: "v1" }, loaded("v1"))
    })
    expect(result.current.result).toMatchObject({
      phase: "complete",
      root: "preview",
      page: { total: 2, offset: 0 },
    })
    const page = result.current.result.phase === "complete" ? result.current.result.page : null
    expect(page?.items.map((item) => item.summary?.cardNo)).toEqual(["P:ONE", "P:TWO"])
  })

  it("pages with offset and limit and reports the full total", () => {
    fake.set({ state: "ready", dataVersion: "v1" }, loaded("v1"))
    const { result } = renderHook(() => useSearch({ ...query, offset: 1, limit: 1 }))
    expect(result.current.result).toMatchObject({
      phase: "complete",
      page: { total: 2, offset: 1, items: [{ key: "c:two" }] },
    })
  })

  it("rejects conditions the synchronous catalog cannot apply instead of ignoring them", () => {
    fake.set({ state: "ready", dataVersion: "v1" }, loaded("v1"))
    const { result } = renderHook(() =>
      useSearch({ ...query, state: { ...DEFAULT_QUERY, cost: { max: 2 }, sort: "cost" } }),
    )
    expect(result.current.result).toEqual({
      phase: "error",
      error: {
        kind: "unsupported-query",
        message: "unsupported conditions",
        facets: ["cost", "sort"],
      },
    })
  })

  it("reports a failed load as load-unavailable, not as an empty result", () => {
    fake.set({ state: "error", kind: "network", detail: "offline" }, null)
    const { result } = renderHook(() => useSearch(query))
    expect(result.current.result).toEqual({
      phase: "error",
      error: { kind: "load-unavailable", message: "network: offline" },
    })
  })

  it("has no result without an input", () => {
    fake.set({ state: "ready", dataVersion: "v1" }, loaded("v1"))
    const { result } = renderHook(() => useSearch(null))
    expect(result.current).toEqual({ lastInput: null, result: { phase: "pending" } })
  })
})
