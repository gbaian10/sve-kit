import { afterEach, describe, expect, it, vi } from "vitest"

import { loadUiLanguage, saveUiLanguage } from "./ui-language"

afterEach(() => {
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
  localStorage.clear()
})

function blocked(): never {
  throw new DOMException("blocked", "SecurityError")
}

describe("ui language setting", () => {
  it("round-trips a saved language", () => {
    expect(loadUiLanguage()).toBeUndefined()
    expect(saveUiLanguage("en")).toBe(true)
    expect(loadUiLanguage()).toBe("en")
  })

  it("ignores a stored value that is not a supported language", () => {
    localStorage.setItem("sve-kit:ui-language", "zh-CN")
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
    const setItem = vi.fn()
    const storage = { getItem: vi.fn(() => "ja"), setItem } as unknown as Storage
    expect(loadUiLanguage(storage)).toBe("ja")
    expect(saveUiLanguage("en", storage)).toBe(true)
    expect(setItem).toHaveBeenCalledWith("sve-kit:ui-language", "en")
  })
})
