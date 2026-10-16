import { afterEach, describe, expect, it, vi } from "vitest"

import {
  createPrefsStore,
  DEFAULT_PREFS,
  PREFS_KEY,
  readPrefs,
  showsOtherEdition,
  writePrefs,
} from "./prefs"

afterEach(() => {
  vi.restoreAllMocks()
  localStorage.clear()
})

function blocked(): never {
  throw new DOMException("blocked", "SecurityError")
}

function stored(): unknown {
  const raw = localStorage.getItem(PREFS_KEY)
  return raw === null ? null : JSON.parse(raw)
}

describe("readPrefs", () => {
  it("returns the defaults when nothing is stored", () => {
    expect(readPrefs()).toEqual(DEFAULT_PREFS)
    expect(DEFAULT_PREFS).toMatchObject({
      uiLanguage: null,
      cardEdition: "jp",
      nameDisplay: "translated",
      effectLanguage: "both",
      symbolLabels: false,
      theme: "system",
      accent: null,
      banRegion: "jp",
      gridDensity: 2,
      viewMode: "grid",
    })
  })

  it("round-trips a full object", () => {
    expect(writePrefs({ ...DEFAULT_PREFS, theme: "dark", accent: "red", gridDensity: 3 })).toBe(
      true,
    )
    expect(readPrefs()).toEqual({ ...DEFAULT_PREFS, theme: "dark", accent: "red", gridDensity: 3 })
    expect(stored()).toMatchObject({ v: 1, theme: "dark" })
  })

  it("falls back per field when a stored value is invalid", () => {
    localStorage.setItem(
      PREFS_KEY,
      JSON.stringify({
        v: 1,
        uiLanguage: "zh-CN",
        cardEdition: "kr",
        theme: "dark",
        accent: "cyan",
        gridDensity: 5,
        viewMode: "table",
        symbolLabels: "yes",
      }),
    )
    expect(readPrefs()).toEqual({ ...DEFAULT_PREFS, theme: "dark", viewMode: "table" })
  })

  it("ignores the removed data-saver field without touching the rest", () => {
    localStorage.setItem(
      PREFS_KEY,
      JSON.stringify({ v: 1, dataSaver: true, termEmphasis: false, theme: "dark" }),
    )
    const prefs = readPrefs()
    expect(prefs).toEqual({ ...DEFAULT_PREFS, termEmphasis: false, theme: "dark" })
    expect(prefs).not.toHaveProperty("dataSaver")
  })

  it.each([
    JSON.stringify({ theme: "dark" }),
    JSON.stringify({ v: 2, theme: "dark" }),
    JSON.stringify({ v: "1", theme: "dark" }),
    JSON.stringify({ v: null, theme: "dark" }),
  ])("treats a missing or different version as another build's data: %s", (raw) => {
    localStorage.setItem(PREFS_KEY, raw)
    expect(readPrefs()).toEqual(DEFAULT_PREFS)
  })

  it.each(["not json", "[]", "null", "42", '"dark"'])(
    "ignores unusable storage content %s",
    (raw) => {
      localStorage.setItem(PREFS_KEY, raw)
      expect(readPrefs()).toEqual(DEFAULT_PREFS)
    },
  )

  it("survives storage that throws", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(blocked)
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(blocked)
    expect(readPrefs()).toEqual(DEFAULT_PREFS)
    expect(writePrefs(DEFAULT_PREFS)).toBe(false)
  })

  it("survives a localStorage getter that throws", () => {
    const descriptor = Object.getOwnPropertyDescriptor(window, "localStorage")
    Object.defineProperty(window, "localStorage", { configurable: true, get: blocked })
    try {
      expect(readPrefs()).toEqual(DEFAULT_PREFS)
      expect(writePrefs(DEFAULT_PREFS)).toBe(false)
    } finally {
      if (descriptor) Object.defineProperty(window, "localStorage", descriptor)
    }
  })
})

describe("createPrefsStore", () => {
  it("reads once, updates by patch, persists and notifies", () => {
    const store = createPrefsStore()
    const listener = vi.fn()
    const unsubscribe = store.subscribe(listener)

    expect(store.get()).toEqual(DEFAULT_PREFS)
    expect(store.set({ theme: "light", accent: "amber" })).toBe(true)
    expect(listener).toHaveBeenCalledTimes(1)
    expect(store.get()).toMatchObject({ theme: "light", accent: "amber" })
    expect(stored()).toMatchObject({ theme: "light", accent: "amber" })

    unsubscribe()
    store.set({ symbolLabels: true })
    expect(listener).toHaveBeenCalledTimes(1)
  })

  it("returns the same snapshot object until something changes", () => {
    const store = createPrefsStore()
    const first = store.get()
    expect(store.get()).toBe(first)
    store.set({ viewMode: "list" })
    expect(store.get()).not.toBe(first)
  })

  it("reload re-reads storage and notifies", () => {
    const store = createPrefsStore()
    const listener = vi.fn()
    store.subscribe(listener)
    writePrefs({ ...DEFAULT_PREFS, viewMode: "table" })
    expect(store.get().viewMode).toBe("grid")
    store.reload()
    expect(store.get().viewMode).toBe("table")
    expect(listener).toHaveBeenCalledTimes(1)
  })

  it("keeps the in-memory value even when persisting fails", () => {
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(blocked)
    const store = createPrefsStore()
    expect(store.set({ banRegion: "en" })).toBe(false)
    expect(store.get().banRegion).toBe("en")
  })

  it("uses an explicitly passed storage", () => {
    const backing = new Map<string, string>()
    const storage = {
      getItem: (key: string) => backing.get(key) ?? null,
      setItem: (key: string, value: string) => {
        backing.set(key, value)
      },
    } as unknown as Storage
    const store = createPrefsStore(() => storage)
    store.set({ cardEdition: "en" })
    expect(backing.has(PREFS_KEY)).toBe(true)
    expect(localStorage.getItem(PREFS_KEY)).toBeNull()
    expect(createPrefsStore(() => storage).get().cardEdition).toBe("en")
  })
})

it.each([true, false])(
  "preserves a legacy shared comparison choice (%s) in both editions",
  (value) => {
    localStorage.setItem(
      PREFS_KEY,
      JSON.stringify({ v: 1, cardEdition: "en", showOtherEdition: value }),
    )
    const prefs = readPrefs()
    expect(showsOtherEdition(prefs, "jp")).toBe(value)
    expect(showsOtherEdition(prefs, "en")).toBe(value)
    writePrefs(prefs)
    expect(readPrefs().showOtherEdition).toEqual({ jp: value, en: value })
  },
)

it.each([undefined, null])(
  "uses the edition defaults when no comparison choice was made (%s)",
  (value) => {
    localStorage.setItem(PREFS_KEY, JSON.stringify({ v: 1, showOtherEdition: value }))
    expect(showsOtherEdition(readPrefs(), "jp")).toBe(false)
    expect(showsOtherEdition(readPrefs(), "en")).toBe(true)
  },
)

it("restores each edition's comparison choice independently", () => {
  localStorage.setItem(
    PREFS_KEY,
    JSON.stringify({ v: 1, showOtherEdition: { jp: true, en: false } }),
  )
  const prefs = readPrefs()
  expect(showsOtherEdition(prefs, "jp")).toBe(true)
  expect(showsOtherEdition(prefs, "en")).toBe(false)
  writePrefs(prefs)
  expect(readPrefs().showOtherEdition).toEqual({ jp: true, en: false })
})
