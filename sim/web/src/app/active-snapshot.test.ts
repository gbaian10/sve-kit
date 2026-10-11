import { renderHook } from "@testing-library/react"
import { afterEach, expect, it, vi } from "vitest"

import { TestSearchWorker } from "../test-utils/search-worker"
import { useActiveSnapshot } from "./snapshot"

afterEach(() => {
  vi.useRealTimers()
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})
it("starts the Worker without loading a full snapshot and rate limits focus checks", () => {
  vi.useFakeTimers({ now: 0, toFake: ["Date"] })
  vi.stubGlobal("Worker", TestSearchWorker)
  const sent = vi.spyOn(TestSearchWorker.prototype, "postMessage")
  const { result } = renderHook(() => useActiveSnapshot())
  expect(result.current.client.snapshot()).toBeNull()
  const loads = () => sent.mock.calls.filter(([request]) => request.kind === "load").length
  expect(loads()).toBe(1)
  window.dispatchEvent(new Event("focus"))
  vi.setSystemTime(59_999)
  window.dispatchEvent(new Event("focus"))
  expect(loads()).toBe(1)
  vi.setSystemTime(60_000)
  window.dispatchEvent(new Event("focus"))
  expect(loads()).toBe(2)
  vi.setSystemTime(60_001)
  window.dispatchEvent(new Event("online"))
  expect(loads()).toBe(3)
  window.dispatchEvent(new Event("focus"))
  expect(loads()).toBe(3)
})
