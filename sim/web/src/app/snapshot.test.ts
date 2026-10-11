import { act, renderHook } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import type { LoadedSnapshot } from "../data"
import { imageSnapshot } from "../test-utils/image-snapshot"
import { useImageIndex } from "./snapshot"

type Fragment = LoadedSnapshot["bootstrap"][number]

class Observer {
  static instances: Observer[] = []
  readonly targets = new Set<Element>()
  disconnected = false
  readonly callback: IntersectionObserverCallback
  readonly options: IntersectionObserverInit | undefined
  constructor(callback: IntersectionObserverCallback, options?: IntersectionObserverInit) {
    this.callback = callback
    this.options = options
    Observer.instances.push(this)
  }
  observe(node: Element) {
    this.targets.add(node)
  }
  unobserve(node: Element) {
    this.targets.delete(node)
  }
  disconnect() {
    this.disconnected = true
    this.targets.clear()
  }
  emit(nodes: Element[], isIntersecting = true) {
    this.callback(
      nodes.map((target) => ({ target, isIntersecting }) as IntersectionObserverEntry),
      this as unknown as IntersectionObserver,
    )
  }
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
function emit(targets: Element[], intersecting = true) {
  for (const observer of Observer.instances.filter((o) => !o.disconnected))
    observer.emit(targets, intersecting)
}
async function collect() {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(110)
  })
}
beforeEach(() => {
  vi.useFakeTimers()
  Observer.instances = []
  vi.stubGlobal("IntersectionObserver", Observer)
})
afterEach(() => {
  container.remove()
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
  vi.useRealTimers()
})

describe("viewport image continuity", () => {
  it("retains URLs immediately after leaving and returning, including a remounted viewer", async () => {
    const targets = nodes(3)
    const { client } = imageSnapshot(3)
    const read = vi.spyOn(client, "fragments")
    const { result, unmount } = renderHook(() => useImageIndex(client, true))
    act(() => {
      emit(targets.slice(0, 2))
    })
    await collect()
    expect(result.current?.cardImage("p:0", "f:0")?.src).toContain("/1.webp")
    act(() => {
      emit(targets.slice(0, 2), false)
      emit(targets.slice(2))
    })
    await collect()
    const reads = read.mock.calls.length
    act(() => {
      emit(targets.slice(2), false)
      emit(targets.slice(0, 2))
    })
    expect(result.current?.known?.("p:0", "f:0")).toBe(true)
    expect(result.current?.cardImage("p:0", "f:0")?.src).toContain("/1.webp")
    await collect()
    expect(read).toHaveBeenCalledTimes(reads)
    unmount()
    const second = renderHook(() => useImageIndex(client, true))
    expect(second.result.current?.cardImage("p:0", "f:0")?.src).toContain("/1.webp")
  })

  it("fetches near faces with height-based pixel margins and updates the range after resize", async () => {
    const targets = nodes(2)
    const { client } = imageSnapshot(2)
    vi.stubGlobal("innerHeight", 800)
    const { result } = renderHook(() => useImageIndex(client, true))
    const near = Observer.instances.find((o) => o.options?.rootMargin)
    expect(near?.options?.rootMargin).toBe("400px 0px 1200px 0px")
    act(() => near?.emit(targets))
    await collect()
    expect(result.current?.cardImage("p:1", "f:1")).toBeDefined()
    expect(result.current?.eager?.("p:1", "f:1")).toBe(false)
    const screen = Observer.instances.find((o) => !o.options?.rootMargin)
    act(() => {
      screen?.emit(targets.slice(0, 1))
    })
    await collect()
    expect(result.current?.eager?.("p:0", "f:0")).toBe(true)
    expect(result.current?.eager?.("p:1", "f:1")).toBe(false)
    vi.stubGlobal("innerHeight", 600)
    act(() => {
      window.dispatchEvent(new Event("resize"))
    })
    expect(near?.disconnected).toBe(true)
    const resized = Observer.instances.find(
      (o) => !o.disconnected && o.options?.rootMargin === "300px 0px 900px 0px",
    )
    expect(resized?.targets.size).toBe(2)
  })

  it("clears displayed URLs on snapshot replacement and ignores a pending old batch", async () => {
    const targets = nodes(2)
    const { client, snapshot, media } = imageSnapshot(2)
    let active: LoadedSnapshot = snapshot
    vi.spyOn(client, "snapshot").mockImplementation(() => active)
    const read = vi.spyOn(client, "fragments")
    const { result, rerender } = renderHook(() => useImageIndex(client, true))
    act(() => {
      emit(targets.slice(0, 1))
    })
    await collect()
    expect(result.current?.cardImage("p:0", "f:0")).toBeDefined()
    let finish: (fragments: readonly Fragment[]) => void = () => undefined
    read.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve
        }),
    )
    act(() => {
      emit(targets.slice(1))
    })
    await collect()
    active = { ...snapshot }
    rerender()
    expect(result.current?.cardImage("p:0", "f:0")).toBeUndefined()
    expect(result.current?.known?.("p:0", "f:0")).toBe(false)
    await act(async () => {
      finish(media)
      await Promise.resolve()
    })
    expect(result.current?.cardImage("p:1", "f:1")).toBeUndefined()
    act(() => {
      emit(targets)
    })
    await collect()
    expect(result.current?.cardImage("p:0", "f:0")).toBeDefined()
    expect(result.current?.cardImage("p:1", "f:1")).toBeDefined()
  })

  it("shares two batch slots across remounts and delivers a started batch to the new viewer", async () => {
    const targets = nodes(36)
    const { client, media } = imageSnapshot(36)
    const finishes: Array<(fragments: readonly Fragment[]) => void> = []
    const read = vi.spyOn(client, "fragments").mockImplementation(
      () =>
        new Promise((resolve) => {
          finishes.push(resolve)
        }),
    )
    const first = renderHook(() => useImageIndex(client, true))
    act(() => {
      emit(targets)
    })
    await collect()
    expect(read).toHaveBeenCalledTimes(2)
    first.unmount()
    const second = renderHook(() => useImageIndex(client, true))
    act(() => {
      emit(targets)
    })
    await collect()
    expect(read).toHaveBeenCalledTimes(2)
    await act(async () => {
      finishes[0]?.(media)
      await Promise.resolve()
    })
    expect(second.result.current?.cardImage("p:0", "f:0")).toBeDefined()
    expect(read).toHaveBeenCalledTimes(3)
    second.unmount()
    await act(async () => {
      finishes[1]?.(media)
      await Promise.resolve()
      finishes[2]?.(media)
      await Promise.resolve()
    })
    expect(read).toHaveBeenCalledTimes(3)
  })

  it("does not start queued faces after the viewer unmounts", async () => {
    const targets = nodes(36)
    const { client, media } = imageSnapshot(36)
    const finishes: Array<(fragments: readonly Fragment[]) => void> = []
    const read = vi.spyOn(client, "fragments").mockImplementation(
      () =>
        new Promise((resolve) => {
          finishes.push(resolve)
        }),
    )
    const { unmount } = renderHook(() => useImageIndex(client, true))
    act(() => {
      emit(targets)
    })
    await collect()
    expect(read).toHaveBeenCalledTimes(2)
    unmount()
    await act(async () => {
      for (const finish of finishes) finish(media)
      await Promise.resolve()
    })
    expect(read).toHaveBeenCalledTimes(2)
  })

  it("fetches the closest faces first and drops faces that leave before a batch starts", async () => {
    const targets = nodes(25)
    const { client, media } = imageSnapshot(25)
    targets.forEach((target, i) => {
      const top = i === 24 ? 100 : 1800 - i * 30
      vi.spyOn(target, "getBoundingClientRect").mockReturnValue({
        top,
        bottom: top + 100,
      } as DOMRect)
    })
    const finishes: Array<(fragments: readonly Fragment[]) => void> = []
    const read = vi.spyOn(client, "fragments").mockImplementation(
      () =>
        new Promise((resolve) => {
          finishes.push(resolve)
        }),
    )
    const { result } = renderHook(() => useImageIndex(client, true))
    act(() => {
      emit(targets)
    })
    await collect()
    act(() => {
      emit(targets.slice(0, 1), false)
    })
    await act(async () => {
      finishes[0]?.(media)
      await Promise.resolve()
    })
    expect(result.current?.cardImage("p:24", "f:24")).toBeDefined()
    expect(result.current?.cardImage("p:0", "f:0")).toBeUndefined()
    expect(read).toHaveBeenCalledTimes(2)
    await act(async () => {
      finishes[1]?.(media)
      await Promise.resolve()
    })
    expect(result.current?.cardImage("p:1", "f:1")).toBeDefined()
    expect(result.current?.cardImage("p:0", "f:0")).toBeUndefined()
  })

  it("records failure for only the attempted faces", async () => {
    const targets = nodes(2)
    const { client } = imageSnapshot(2)
    vi.spyOn(client, "fragments").mockRejectedValueOnce(new Error("metadata unavailable"))
    const { result } = renderHook(() => useImageIndex(client, true))
    act(() => {
      emit(targets.slice(0, 1))
    })
    await collect()
    expect(result.current?.known?.("p:0", "f:0")).toBe(true)
    expect(result.current?.failedFace?.("p:0", "f:0")).toBe(true)
    expect(result.current?.known?.("p:1", "f:1")).toBe(false)
    expect(result.current?.failedFace?.("p:1", "f:1")).toBe(false)
  })
})
