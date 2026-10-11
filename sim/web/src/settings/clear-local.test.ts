import { afterEach, describe, expect, it, vi } from "vitest"

import { clearLocalData } from "./clear-local"

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
        deleted.push(`db:${name}`)
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
})
