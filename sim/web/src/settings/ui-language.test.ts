import { afterEach, describe, expect, it, vi } from "vitest"

import { LEGACY_UI_LANGUAGE_KEY, PREFS_KEY, prefsStore, readPrefs } from "./prefs"
import { loadUiLanguage, saveUiLanguage } from "./ui-language"

afterEach(() => {
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
  localStorage.clear()
  prefsStore.reload()
})

function blocked(): never {
  throw new DOMException("blocked", "SecurityError")
}

describe("ui language setting", () => {
  it("round-trips a saved language through the prefs object", () => {
    expect(loadUiLanguage()).toBeUndefined()
    expect(saveUiLanguage("en")).toBe(true)
    expect(loadUiLanguage()).toBe("en")
    expect(JSON.parse(localStorage.getItem(PREFS_KEY) ?? "{}")).toMatchObject({ uiLanguage: "en" })
  })

  it("goes through the shared store, so a later set() keeps the language and subscribers hear it", () => {
    const listener = vi.fn()
    const unsubscribe = prefsStore.subscribe(listener)
    expect(saveUiLanguage("en")).toBe(true)
    expect(listener).toHaveBeenCalledTimes(1)
    expect(prefsStore.get().uiLanguage).toBe("en")
    prefsStore.set({ theme: "dark" })
    expect(readPrefs()).toMatchObject({ uiLanguage: "en", theme: "dark" })
    expect(loadUiLanguage()).toBe("en")
    unsubscribe()
  })

  it("still reads the legacy key and ignores unsupported values", () => {
    localStorage.setItem(LEGACY_UI_LANGUAGE_KEY, "ja")
    prefsStore.reload()
    expect(loadUiLanguage()).toBe("ja")
    localStorage.setItem(LEGACY_UI_LANGUAGE_KEY, "zh-CN")
    prefsStore.reload()
    expect(loadUiLanguage()).toBeUndefined()
  })

  it("survives getItem and setItem that throw", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(blocked)
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(blocked)
    expect(loadUiLanguage()).toBeUndefined()
    expect(saveUiLanguage("ja")).toBe(false)
  })

  it("survives a localStorage getter that throws", () => {
    const descriptor = Object.getOwnPropertyDescriptor(window, "localStorage")
    Object.defineProperty(window, "localStorage", { configurable: true, get: blocked })
    try {
      expect(() => localStorage).toThrow("blocked")
      expect(loadUiLanguage()).toBeUndefined()
      expect(saveUiLanguage("ja")).toBe(false)
    } finally {
      if (descriptor) Object.defineProperty(window, "localStorage", descriptor)
    }
  })

  it("uses an explicitly passed storage", () => {
    const backing = new Map<string, string>()
    const storage = {
      getItem: (key: string) => backing.get(key) ?? null,
      setItem: (key: string, value: string) => {
        backing.set(key, value)
      },
    } as unknown as Storage
    expect(saveUiLanguage("en", storage)).toBe(true)
    expect(loadUiLanguage(storage)).toBe("en")
    expect(localStorage.getItem(PREFS_KEY)).toBeNull()
  })
})
