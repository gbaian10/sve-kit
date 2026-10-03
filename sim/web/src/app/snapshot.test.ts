import { act, renderHook, waitFor } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { type ImageIndex, type LoadedSnapshot, loadImagePage, type SnapshotClient } from "../data"
import { useImageIndex } from "./snapshot"

vi.mock("../data", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../data")>()),
  loadImagePage: vi.fn(),
}))
let emit: (nodes: Element[]) => void = () => undefined
class Observer {
  constructor(callback: IntersectionObserverCallback) {
    emit = (nodes) => {
      callback(
        nodes.map((target) => ({ target, isIntersecting: true }) as IntersectionObserverEntry),
        this as unknown as IntersectionObserver,
      )
    }
  }
  observe() {}
  unobserve() {}
  disconnect() {}
}
let container: HTMLDivElement
function nodes(count: number): Element[] {
  container = document.createElement("div")
  for (let i = 0; i < count; i += 1) {
    const node = document.createElement("span")
    node.dataset["imagePrinting"] = `p:${String(i)}`
    node.dataset["imageFace"] = `f:${String(i)}`
    container.append(node)
  }
  document.body.append(container)
  return [...container.children]
}
const index = (ids: string[]): ImageIndex => ({
  asset: () => undefined,
  cardImage: (id) =>
    ids.includes(id) ? { src: `/${id}.webp`, srcSet: "", width: 1, height: 1 } : undefined,
})
afterEach(() => {
  container.remove()
  vi.clearAllMocks()
  vi.unstubAllGlobals()
})
describe("viewport image continuity", () => {
  it("keeps overlapping images while newly visible faces load and rejects old snapshot display", async () => {
    vi.stubGlobal("IntersectionObserver", Observer)
    const targets = nodes(3)
    let snapshot = {} as LoadedSnapshot
    const client = { snapshot: () => snapshot } as SnapshotClient
    vi.mocked(loadImagePage)
      .mockResolvedValue(index([]))
      .mockResolvedValueOnce(index(["p:0", "p:1"]))
    const { result, rerender } = renderHook(() => useImageIndex(client, true))
    act(() => {
      emit(targets.slice(0, 2))
    })
    await waitFor(() => {
      expect(result.current?.cardImage("p:1", "f:1")).toBeDefined()
    })
    let resolve: (value: ImageIndex) => void = () => undefined
    vi.mocked(loadImagePage).mockImplementationOnce(
      () =>
        new Promise((done) => {
          resolve = done
        }),
    )
    act(() => {
      emit(targets)
    })
    await waitFor(() => {
      expect(loadImagePage).toHaveBeenCalledTimes(2)
    })
    expect(result.current?.cardImage("p:1", "f:1")).toBeDefined()
    await act(async () => {
      resolve(index(["p:0", "p:1", "p:2"]))
      await Promise.resolve()
    })
    expect(result.current?.cardImage("p:2", "f:2")).toBeDefined()
    snapshot = {} as LoadedSnapshot
    rerender()
    expect(result.current).toBeUndefined()
  })
  it("debounces observer bursts and bounds a larger viewport to 24 unique faces", async () => {
    vi.stubGlobal("IntersectionObserver", Observer)
    const targets = nodes(30)
    const snapshot = {} as LoadedSnapshot
    const client = { snapshot: () => snapshot } as SnapshotClient
    vi.mocked(loadImagePage).mockResolvedValue(index([]))
    renderHook(() => useImageIndex(client, true))
    act(() => {
      emit(targets.slice(0, 10))
      emit(targets.slice(0, 20))
      emit(targets)
    })
    await waitFor(() => {
      expect(loadImagePage).toHaveBeenCalledOnce()
    })
    expect(vi.mocked(loadImagePage).mock.calls[0]?.[1]).toHaveLength(24)
  })
})
