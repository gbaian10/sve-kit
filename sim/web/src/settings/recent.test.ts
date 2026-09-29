import { describe, expect, it } from "vitest"

import { createRecentStore, readRecent, RECENT_KEY } from "./recent"

class MemoryStorage implements Storage {
  private readonly map = new Map<string, string>()
  get length() {
    return this.map.size
  }
  clear() {
    this.map.clear()
  }
  getItem(key: string) {
    return this.map.get(key) ?? null
  }
  key(index: number) {
    return [...this.map.keys()][index] ?? null
  }
  removeItem(key: string) {
    this.map.delete(key)
  }
  setItem(key: string, value: string) {
    this.map.set(key, value)
  }
}

describe("recent store", () => {
  it("keeps the newest first, de-duplicates and caps at eight", () => {
    const storage = new MemoryStorage()
    const store = createRecentStore(storage)
    for (let i = 1; i <= 9; i += 1) store.push(`p:${String(i)}`)
    store.push("p:5")
    expect(store.get()).toEqual(["p:5", "p:9", "p:8", "p:7", "p:6", "p:4", "p:3", "p:2"])
    expect(readRecent(storage)).toEqual(store.get())
    store.clear()
    expect(readRecent(storage)).toEqual([])
  })

  it("ignores broken storage contents and storage failures", () => {
    const storage = new MemoryStorage()
    storage.setItem(RECENT_KEY, '{"not":"a list"}')
    expect(readRecent(storage)).toEqual([])
    storage.setItem(RECENT_KEY, '["a", 1, "", "a", "b"]')
    expect(readRecent(storage)).toEqual(["a", "b"])
    const broken = createRecentStore(() => {
      throw new Error("blocked")
    })
    broken.push("p:1")
    expect(broken.get()).toEqual(["p:1"])
  })
})
