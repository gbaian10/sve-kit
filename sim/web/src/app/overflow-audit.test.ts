import { afterEach, describe, expect, it, vi } from "vitest"

import { describeOverflow, findOverflow, installOverflowAudit } from "./overflow-audit"

interface Box {
  readonly scroll: number
  readonly client: number
  readonly style?: Partial<Pick<CSSStyleDeclaration, "overflowX" | "whiteSpace" | "textOverflow">>
}

const styles = new Map<Element, Box["style"]>()

function box(tag: string, { scroll, client, style }: Box, parent: Element = document.body) {
  const element = document.createElement(tag)
  Object.defineProperty(element, "scrollWidth", { value: scroll, configurable: true })
  Object.defineProperty(element, "clientWidth", { value: client, configurable: true })
  styles.set(element, style)
  parent.append(element)
  return element
}

const readStyle = (element: Element) =>
  ({
    overflowX: "visible",
    whiteSpace: "normal",
    textOverflow: "clip",
    ...styles.get(element),
  }) as CSSStyleDeclaration

afterEach(() => {
  document.body.replaceChildren()
  styles.clear()
  vi.useRealTimers()
  vi.restoreAllMocks()
})

describe("findOverflow", () => {
  it("reports nowrap text, clipped boxes and visible overflow, innermost only", () => {
    const row = box("div", { scroll: 300, client: 280 })
    const pill = box("button", { scroll: 90, client: 70, style: { whiteSpace: "nowrap" } }, row)
    pill.textContent = "Traditional Chinese"
    const panel = box("section", { scroll: 340, client: 320, style: { overflowX: "hidden" } })
    box("p", { scroll: 200, client: 150 })
    const kinds = findOverflow(document, readStyle).map(describeOverflow)
    expect(kinds).toEqual([
      { kind: "nowrap", tag: "button", className: "", text: "Traditional Chinese", overflow: 20 },
      { kind: "clipped", tag: "section", className: "", text: "", overflow: 20 },
      { kind: "visible", tag: "p", className: "", text: "", overflow: 50 },
    ])
    expect(panel.isConnected).toBe(true)
  })

  it("ignores scroll areas, ellipsis truncation and one-pixel rounding", () => {
    box("div", { scroll: 500, client: 300, style: { overflowX: "auto" } })
    box("div", { scroll: 500, client: 300, style: { overflowX: "scroll" } })
    box("span", {
      scroll: 120,
      client: 80,
      style: { whiteSpace: "nowrap", textOverflow: "ellipsis", overflowX: "hidden" },
    })
    box("div", { scroll: 101, client: 100 })
    box("a", { scroll: 80, client: 1, style: { whiteSpace: "nowrap", overflowX: "hidden" } })
    expect(findOverflow(document, readStyle)).toEqual([])
  })

  it("reports the page itself when the document scrolls sideways", () => {
    Object.defineProperty(document.documentElement, "scrollWidth", {
      value: 420,
      configurable: true,
    })
    Object.defineProperty(document.documentElement, "clientWidth", {
      value: 390,
      configurable: true,
    })
    expect(findOverflow(document, readStyle).map(describeOverflow)).toEqual([
      { kind: "page", tag: "html", className: "", text: "", overflow: 30 },
    ])
    Object.defineProperty(document.documentElement, "scrollWidth", { value: 0, configurable: true })
    Object.defineProperty(document.documentElement, "clientWidth", { value: 0, configurable: true })
  })
})

describe("installOverflowAudit", () => {
  it("logs each overflowing element once and exposes an on-demand report", async () => {
    vi.useFakeTimers()
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined)
    const uninstall = installOverflowAudit()
    expect(typeof window.__sveOverflowAudit).toBe("function")
    await vi.advanceTimersByTimeAsync(300)
    expect(error).not.toHaveBeenCalled()
    // Real layout is unavailable in jsdom, so widths are stubbed on the element.
    const pill = box("button", { scroll: 90, client: 70 })
    pill.textContent = "Follow system"
    vi.spyOn(window, "getComputedStyle").mockImplementation((element) => readStyle(element))
    styles.set(pill, { whiteSpace: "nowrap" })
    await vi.advanceTimersByTimeAsync(300)
    expect(error).toHaveBeenCalledTimes(1)
    expect(error.mock.calls[0]?.[0]).toContain(
      'nowrap: <button> is 20px too narrow for "Follow system"',
    )
    expect(window.__sveOverflowAudit?.()).toEqual([
      { kind: "nowrap", tag: "button", className: "", text: "Follow system", overflow: 20 },
    ])
    pill.setAttribute("data-touch", "again")
    await vi.advanceTimersByTimeAsync(300)
    expect(error).toHaveBeenCalledTimes(1)
    // Recovered, then the same overflow again: that is a new event and gets logged again.
    Object.defineProperty(pill, "scrollWidth", { value: 70, configurable: true })
    pill.setAttribute("data-touch", "recovered")
    await vi.advanceTimersByTimeAsync(300)
    Object.defineProperty(pill, "scrollWidth", { value: 90, configurable: true })
    pill.setAttribute("data-touch", "overflowing")
    await vi.advanceTimersByTimeAsync(300)
    expect(error).toHaveBeenCalledTimes(2)
    uninstall()
    expect(window.__sveOverflowAudit).toBeUndefined()
  })
})
