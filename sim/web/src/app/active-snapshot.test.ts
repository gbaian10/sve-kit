import { act, renderHook, waitFor } from "@testing-library/react"
import { createElement, type ReactNode } from "react"
import { MemoryRouter } from "react-router"
import { afterEach, expect, it, vi } from "vitest"

import { buildSnapshot } from "../../scripts/fixture/build"
import { appBackgroundDownloads, objectValue, parseJson, setActiveSnapshotRoot } from "../data"
import { DEFAULT_PREFS, prefsStore } from "../settings"
import { memoryCache } from "../test-utils/cache"
import { TestSearchWorker } from "../test-utils/search-worker"
import { useActiveSnapshot } from "./snapshot"

afterEach(() => {
  vi.useRealTimers()
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})
it("starts one Worker, follows app selections and preferences, and rate limits focus checks", async () => {
  const built = await buildSnapshot({ encodeImage: () => Promise.resolve(new Uint8Array([1])) })
  const pointer = built.files.get("snapshots/versions/index.json")
  if (!pointer) throw new Error("index missing")
  built.files.set(
    "snapshots/preview/current.json",
    new TextEncoder().encode(JSON.stringify(objectValue(parseJson(pointer))["current"])),
  )
  prefsStore.set(DEFAULT_PREFS)
  vi.stubGlobal("caches", memoryCache())
  vi.stubGlobal("fetch", (url: string) => {
    const body = built.files.get(url.replace(/^.*\/cdn(?:-preview)?\//u, ""))
    return Promise.resolve(
      body ? new Response(body.slice().buffer) : new Response(null, { status: 404 }),
    )
  })
  vi.useFakeTimers({ now: 0, toFake: ["Date"] })
  vi.stubGlobal("Worker", TestSearchWorker)
  const sent = vi.spyOn(TestSearchWorker.prototype, "postMessage")
  const { result } = renderHook(() => useActiveSnapshot(), {
    wrapper: ({ children }: { children: ReactNode }) => createElement(MemoryRouter, null, children),
  })
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
  await waitFor(() => {
    expect(result.current.status.state).toBe("ready")
  })
  await waitFor(() => {
    expect(appBackgroundDownloads.getStatus().selection?.edition).toBe("jp")
  })
  act(() => {
    prefsStore.set({ showOtherEdition: { jp: true, en: false } })
  })
  await waitFor(() => {
    expect(appBackgroundDownloads.getStatus().selection?.edition).toBe("en")
  })
  act(() => {
    prefsStore.set({ uiLanguage: "ja", backgroundDownloads: false })
  })
  expect(appBackgroundDownloads.getStatus()).toMatchObject({
    state: "disabled",
    selection: { edition: "en" },
  })
  act(() => {
    setActiveSnapshotRoot("preview")
  })
  expect(appBackgroundDownloads.getStatus().selection).toBeUndefined()
  await waitFor(() => {
    expect(result.current.status.state).toBe("ready")
    expect(result.current.root).toBe("preview")
  })
  expect(appBackgroundDownloads.getStatus().selection?.base).toBe(result.current.client.base)
  act(() => {
    setActiveSnapshotRoot("cdn")
    prefsStore.set(DEFAULT_PREFS)
  })
})
