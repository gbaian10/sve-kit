import { act, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState } from "react"
import { afterEach, beforeEach, expect, it, vi } from "vitest"

import { renderInRouter } from "../../test-utils"
import { PaginationTail } from "./PaginationTail"

let intersect: IntersectionObserverCallback
let disconnected = false
let root: IntersectionObserverInit | undefined
beforeEach(() => {
  disconnected = false
  vi.stubGlobal(
    "IntersectionObserver",
    class {
      constructor(callback: IntersectionObserverCallback, options: IntersectionObserverInit) {
        intersect = callback
        root = options
      }
      observe() {}
      disconnect() {
        disconnected = true
      }
    },
  )
})
afterEach(() => vi.unstubAllGlobals())
function nearBottom() {
  act(() => {
    intersect([{ isIntersecting: true } as IntersectionObserverEntry], {} as IntersectionObserver)
  })
}

it("loads once near the bottom, blocks during loading and stops at the total", async () => {
  const onMore = vi.fn()
  function Example() {
    const [phase, setPhase] = useState(0)
    return (
      <>
        <button
          onClick={() => {
            setPhase((phase) => phase + 1)
          }}
        >
          Advance
        </button>
        <PaginationTail
          requestKey={String(phase)}
          total={120}
          hasMore={phase < 2}
          loading={phase === 1}
          failed={false}
          onMore={onMore}
          onRetry={vi.fn()}
        />
      </>
    )
  }
  await renderInRouter(<Example />)
  expect(root?.root).toBeNull()
  expect(root?.rootMargin).toBe(`0px 0px ${String(window.innerHeight)}px 0px`)
  nearBottom()
  nearBottom()
  expect(onMore).toHaveBeenCalledTimes(1)
  const user = userEvent.setup()
  await user.click(screen.getByRole("button", { name: "Advance" }))
  expect(screen.getByRole("status")).toHaveTextContent("載入更多卡片中")
  nearBottom()
  expect(onMore).toHaveBeenCalledTimes(1)
  await user.click(screen.getByRole("button", { name: "Advance" }))
  expect(screen.getByText("共 120 張")).toBeVisible()
  nearBottom()
  expect(onMore).toHaveBeenCalledTimes(1)
  expect(disconnected).toBe(true)
  expect(screen.queryByRole("button", { name: "載入更多" })).toBeNull()
})

it("offers retry without triggering more pages after an error", async () => {
  const onMore = vi.fn(),
    onRetry = vi.fn()
  await renderInRouter(
    <PaginationTail
      requestKey="failed"
      total={120}
      hasMore
      loading={false}
      failed
      onMore={onMore}
      onRetry={onRetry}
    />,
  )
  expect(screen.getByText("載入失敗")).toBeVisible()
  await userEvent.click(screen.getByRole("button", { name: "重試" }))
  expect(onRetry).toHaveBeenCalledOnce()
  expect(onMore).not.toHaveBeenCalled()
})

it("observes the narrow list scrollport instead of the page", async () => {
  await renderInRouter(
    <div data-list-scroll="" style={{ overflowY: "auto" }}>
      <PaginationTail
        requestKey="narrow"
        total={120}
        hasMore
        loading={false}
        failed={false}
        onMore={vi.fn()}
        onRetry={vi.fn()}
      />
    </div>,
  )
  expect(root?.root).toBe(document.querySelector("[data-list-scroll]"))
})
