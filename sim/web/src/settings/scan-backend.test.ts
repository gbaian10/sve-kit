import { afterEach, describe, expect, it, vi } from "vitest"

import { savedScanBackend, saveScanBackend } from "./scan-backend"

afterEach(() => {
  localStorage.clear()
  vi.restoreAllMocks()
})

describe("scan backend choice", () => {
  it("remembers the choice on this device and ignores unknown values", () => {
    expect(savedScanBackend()).toBeUndefined()
    saveScanBackend("cpu")
    expect(savedScanBackend()).toBe("cpu")
    localStorage.setItem("sve-kit:scan-backend", "metal")
    expect(savedScanBackend()).toBeUndefined()
  })

  it("works when storage is blocked", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked")
    })
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked")
    })
    expect(savedScanBackend()).toBeUndefined()
    expect(() => {
      saveScanBackend("webgpu")
    }).not.toThrow()
  })
})
