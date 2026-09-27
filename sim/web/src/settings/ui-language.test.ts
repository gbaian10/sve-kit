import { afterEach, describe, expect, it, vi } from "vitest"

import { loadUiLanguage, saveUiLanguage } from "./ui-language"

afterEach(() => {
  localStorage.clear()
  vi.restoreAllMocks()
})

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

  it("survives storage that throws", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new DOMException("blocked", "SecurityError")
    })
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new DOMException("blocked", "SecurityError")
    })
    expect(loadUiLanguage()).toBeUndefined()
    expect(saveUiLanguage("ja")).toBe(false)
  })
})
