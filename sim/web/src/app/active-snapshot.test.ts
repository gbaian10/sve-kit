import { renderHook } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { useActiveSnapshot } from "./snapshot"

const client = vi.hoisted(() => {
  // useSyncExternalStore needs the same status object until the store changes.
  const status = { state: "loading", phase: "index" }
  return {
    load: vi.fn(() => Promise.resolve()),
    reload: vi.fn(() => Promise.resolve()),
    subscribe: () => () => undefined,
    status: () => status,
    snapshot: () => null,
  }
})

vi.mock("../data", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../data")>()),
  snapshotClient: () => client,
}))

describe("useActiveSnapshot", () => {
  afterEach(() => {
    vi.useRealTimers()
    vi.clearAllMocks()
  })

  it("checks for a new version on focus at most once a minute and at once when back online", () => {
    vi.useFakeTimers({ now: 0, toFake: ["Date"] })
    renderHook(() => useActiveSnapshot())
    expect(client.load).toHaveBeenCalledTimes(1)
    window.dispatchEvent(new Event("focus"))
    vi.setSystemTime(59_999)
    window.dispatchEvent(new Event("focus"))
    expect(client.reload).not.toHaveBeenCalled()
    vi.setSystemTime(60_000)
    window.dispatchEvent(new Event("focus"))
    expect(client.reload).toHaveBeenCalledTimes(1)
    vi.setSystemTime(60_001)
    window.dispatchEvent(new Event("online"))
    expect(client.reload).toHaveBeenCalledTimes(2)
    // The online check also counts as the latest check for the focus interval.
    window.dispatchEvent(new Event("focus"))
    expect(client.reload).toHaveBeenCalledTimes(2)
  })
})
