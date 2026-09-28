import { afterEach, describe, expect, it, vi } from "vitest"

import { loadRecent, pushRecent, RECENT_KEY, RECENT_LIMIT } from "./recent"

afterEach(() => {
  vi.restoreAllMocks()
  localStorage.clear()
})

describe("recently viewed", () => {
  it("starts empty and keeps the newest first", () => {
    expect(loadRecent()).toEqual([])
    pushRecent("BP01-001")
    pushRecent("BP01-002")
    expect(loadRecent()).toEqual(["BP01-002", "BP01-001"])
  })

  it("moves a repeated id to the front instead of duplicating it", () => {
    pushRecent("a")
    pushRecent("b")
    pushRecent("a")
    expect(loadRecent()).toEqual(["a", "b"])
  })

  it(`keeps at most ${String(RECENT_LIMIT)} entries`, () => {
    for (let i = 0; i < RECENT_LIMIT + 3; i += 1) pushRecent(`id-${String(i)}`)
    const list = loadRecent()
    expect(list).toHaveLength(RECENT_LIMIT)
    expect(list[0]).toBe(`id-${String(RECENT_LIMIT + 2)}`)
  })

  it("also caps and de-duplicates data written by another build when reading", () => {
    const stale = ["a", "b", "a", "c", "d", "e", "f", "g", "h", "i", "j"]
    localStorage.setItem(RECENT_KEY, JSON.stringify(stale))
    expect(loadRecent()).toEqual(["a", "b", "c", "d", "e", "f", "g", "h"])
  })

  it("ignores stored content that is not a list of strings", () => {
    localStorage.setItem(RECENT_KEY, JSON.stringify({ a: 1 }))
    expect(loadRecent()).toEqual([])
    localStorage.setItem(RECENT_KEY, JSON.stringify(["ok", 3, null]))
    expect(loadRecent()).toEqual(["ok"])
    localStorage.setItem(RECENT_KEY, "{oops")
    expect(loadRecent()).toEqual([])
  })

  it("survives storage that throws", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new DOMException("blocked", "SecurityError")
    })
    expect(loadRecent()).toEqual([])
    expect(pushRecent("x")).toEqual(["x"])
  })
})
