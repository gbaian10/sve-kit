import { afterEach, describe, expect, it, vi } from "vitest"

import { MIN_AIM_SCALE, saveAimScale, savedAimScale } from "./scan-aim"

afterEach(() => {
  localStorage.clear()
  vi.restoreAllMocks()
})

describe("scan aiming frame size", () => {
  it("starts full size and remembers the size within its range", () => {
    expect(savedAimScale()).toBe(1)
    saveAimScale(0.6)
    expect(savedAimScale()).toBe(0.6)
    saveAimScale(0.1)
    expect(savedAimScale()).toBe(MIN_AIM_SCALE)
    localStorage.setItem("sve-kit:scan-aim", "big")
    expect(savedAimScale()).toBe(1)
  })

  it("works when storage is blocked", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked")
    })
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked")
    })
    expect(savedAimScale()).toBe(1)
    expect(() => {
      saveAimScale(0.5)
    }).not.toThrow()
  })
})
