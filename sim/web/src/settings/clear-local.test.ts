import { afterEach, describe, expect, it, vi } from "vitest"

import { memoryCache } from "../test-utils/cache"
import {
  clearLocalData,
  getCacheClearResult,
  requestColdStart,
  runRequestedColdStart,
} from "./clear-local"

afterEach(() => {
  vi.unstubAllGlobals()
})

describe("clearLocalData", () => {
  it("drops caches, databases, workers and stored data but keeps settings", async () => {
    const deleted: string[] = []
    vi.stubGlobal("caches", {
      keys: () => Promise.resolve(["snapshots", "images"]),
      delete: (name: string) => {
        deleted.push(`cache:${name}`)
        return Promise.resolve(true)
      },
    })
    vi.stubGlobal("indexedDB", {
      databases: () => Promise.resolve([{ name: "search" }, {}]),
      deleteDatabase: (name: string) => {
        const request: { onsuccess?: () => void } = {}
        // The deletion only counts once the request reports success.
        setTimeout(() => {
          deleted.push(`db:${name}`)
          request.onsuccess?.()
        }, 5)
        return request
      },
    })
    const unregister = vi.fn(() => Promise.resolve(true))
    Object.defineProperty(navigator, "serviceWorker", {
      configurable: true,
      value: { getRegistrations: () => Promise.resolve([{ unregister }]) },
    })
    localStorage.setItem("sve-kit:prefs", "{}")
    localStorage.setItem("sve-kit:recent", "[]")
    localStorage.setItem("sve-kit:index-revision", "3")
    sessionStorage.setItem("x", "1")
    await clearLocalData()
    expect(deleted.sort()).toEqual(["cache:images", "cache:snapshots", "db:search"])
    expect(unregister).toHaveBeenCalledOnce()
    expect(Object.keys(localStorage).sort()).toEqual(["sve-kit:prefs", "sve-kit:recent"])
    expect(sessionStorage.length).toBe(0)
    Reflect.deleteProperty(navigator, "serviceWorker")
    localStorage.clear()
  })

  it("clears only at the start that follows a request", async () => {
    const keys = vi.fn(() => Promise.resolve([]))
    vi.stubGlobal("caches", { keys, delete: () => Promise.resolve(true) })
    localStorage.setItem("sve-kit:index-revision", "3")
    await runRequestedColdStart()
    expect(keys).not.toHaveBeenCalled()
    expect(localStorage.getItem("sve-kit:index-revision")).toBe("3")
    requestColdStart()
    await runRequestedColdStart()
    expect(keys).toHaveBeenCalledOnce()
    expect(localStorage.getItem("sve-kit:index-revision")).toBeNull()
    expect(localStorage.getItem("sve-kit:cold-start")).toBeNull()
    await runRequestedColdStart()
    expect(keys).toHaveBeenCalledOnce()
  })
})

it("clears managed caches on the requested next start, keeping preferences, recent and other storage", async () => {
  const storage = memoryCache()
  vi.stubGlobal("caches", storage)
  await storage.open("sve-image-responses-v2")
  await storage.open("sve-search-index:root")
  await storage.open("unrelated")
  localStorage.setItem("sve-kit:prefs", '{"theme":"dark"}')
  localStorage.setItem("sve-kit:recent", '["synthetic"]')
  sessionStorage.setItem("unrelated", "keep")
  requestColdStart("caches")
  expect(await storage.keys()).toHaveLength(3)
  await runRequestedColdStart()
  expect(await storage.keys()).toEqual(["unrelated"])
  expect(localStorage.getItem("sve-kit:prefs")).toBe('{"theme":"dark"}')
  expect(localStorage.getItem("sve-kit:recent")).toBe('["synthetic"]')
  expect(sessionStorage.getItem("unrelated")).toBe("keep")
  expect(getCacheClearResult()).toBe("cleared")
  expect(localStorage.getItem("sve-kit:cold-start")).toBeNull()
  localStorage.clear()
  sessionStorage.clear()
})

it("exposes failure at startup instead of reporting successful cache clearing", async () => {
  vi.stubGlobal("caches", { keys: () => Promise.reject(new Error("denied")) })
  requestColdStart("caches")
  await runRequestedColdStart()
  expect(getCacheClearResult()).toBe("failed")
  expect(localStorage.getItem("sve-kit:cold-start")).toBeNull()
})
