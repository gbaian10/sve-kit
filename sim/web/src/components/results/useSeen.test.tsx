import { act, render, screen } from "@testing-library/react"
import { useRef } from "react"
import { afterEach, expect, it, vi } from "vitest"

import { useSeen } from "./useSeen"

function Preview({ name }: { readonly name: string }) {
  const ref = useRef<HTMLDivElement>(null)
  const seen = useSeen(ref)
  return (
    <div ref={ref}>
      {name}: {seen ? "ready" : "waiting"}
    </div>
  )
}

afterEach(() => vi.unstubAllGlobals())

it("shares a viewport-height preload margin, follows resizes and resolves each row once", () => {
  const observers: Observer[] = []
  class Observer {
    readonly targets = new Set<Element>()
    readonly disconnect = vi.fn(() => {
      this.targets.clear()
    })
    readonly notify: (entries: { target: Element; isIntersecting: boolean }[]) => void
    readonly options: IntersectionObserverInit
    constructor(
      notify: (entries: { target: Element; isIntersecting: boolean }[]) => void,
      options: IntersectionObserverInit,
    ) {
      this.notify = notify
      this.options = options
      observers.push(this)
    }
    observe(target: Element) {
      this.targets.add(target)
    }
    unobserve(target: Element) {
      this.targets.delete(target)
    }
  }
  vi.stubGlobal("IntersectionObserver", Observer)
  vi.stubGlobal("innerHeight", 844)
  const view = render(
    <>
      <Preview name="First" />
      <Preview name="Second" />
    </>,
  )
  expect(observers).toHaveLength(1)
  expect(observers[0]?.options.rootMargin).toBe("844px 0px 844px 0px")
  const first = screen.getByText("First: waiting")
  act(() => {
    observers[0]?.notify([{ target: first, isIntersecting: false }])
  })
  expect(first).toHaveTextContent("waiting")
  vi.stubGlobal("innerHeight", 600)
  act(() => {
    window.dispatchEvent(new Event("resize"))
  })
  const resized = observers[1]
  expect(resized?.options.rootMargin).toBe("600px 0px 600px 0px")
  expect(resized?.targets.size).toBe(2)
  act(() => {
    resized?.notify([{ target: first, isIntersecting: true }])
  })
  expect(screen.getByText("First: ready")).toBeVisible()
  expect(resized?.targets.size).toBe(1)
  view.unmount()
  expect(resized?.disconnect).toHaveBeenCalled()
  act(() => {
    window.dispatchEvent(new Event("resize"))
  })
  expect(observers).toHaveLength(2)
})

it("activates a screen of previews in separate tasks and cancels queued rows on unmount", async () => {
  vi.useFakeTimers()
  let notify: ((entries: { target: Element; isIntersecting: boolean }[]) => void) | undefined
  vi.stubGlobal(
    "IntersectionObserver",
    class {
      constructor(callback: (entries: { target: Element; isIntersecting: boolean }[]) => void) {
        notify = callback
      }
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  )
  const view = render(
    <>
      <Preview name="First" />
      <Preview name="Second" />
      <Preview name="Third" />
    </>,
  )
  const targets = ["First", "Second", "Third"].map((name) => screen.getByText(`${name}: waiting`))
  act(() => {
    notify?.(targets.map((target) => ({ target, isIntersecting: true })))
  })
  expect(screen.getByText("First: ready")).toBeVisible()
  expect(screen.getByText("Second: waiting")).toBeVisible()
  expect(screen.getByText("Third: waiting")).toBeVisible()
  await act(async () => {
    await vi.advanceTimersToNextTimerAsync()
  })
  expect(screen.getByText("Second: ready")).toBeVisible()
  expect(screen.getByText("Third: waiting")).toBeVisible()
  view.unmount()
  await act(async () => {
    await vi.runAllTimersAsync()
  })
  expect(screen.queryByText("Third: ready")).not.toBeInTheDocument()
  vi.useRealTimers()
})
