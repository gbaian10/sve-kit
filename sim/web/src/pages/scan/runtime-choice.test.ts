import { afterEach, describe, expect, it, vi } from "vitest"

import { saveScanBackend } from "../../settings"
import { currentBackend, neededParts } from "./runtime-choice"

function setGpu(gpu: unknown) {
  Object.defineProperty(navigator, "gpu", { value: gpu, configurable: true })
}

afterEach(() => {
  Reflect.deleteProperty(navigator, "gpu")
  vi.restoreAllMocks()
})

describe("runtime choice", () => {
  it("defaults to CPU when the browser has no WebGPU", async () => {
    expect(await currentBackend()).toBe("cpu")
  })

  it("defaults to WebGPU only when an adapter is actually available", async () => {
    setGpu({ requestAdapter: () => Promise.resolve({}) })
    expect(await currentBackend()).toBe("webgpu")
    setGpu({ requestAdapter: () => Promise.resolve(null) })
    expect(await currentBackend()).toBe("cpu")
    setGpu({ requestAdapter: () => Promise.reject(new Error("blocked")) })
    expect(await currentBackend()).toBe("cpu")
  })

  it("keeps the user's saved choice over detection", async () => {
    setGpu({ requestAdapter: () => Promise.resolve({}) })
    saveScanBackend("cpu")
    expect(await currentBackend()).toBe("cpu")
  })

  it("needs only the chosen runtime, the finder and the recogniser", () => {
    expect(neededParts("webgpu")).toEqual(["runtime-webgpu", "det", "rec"])
    expect(neededParts("cpu")).toEqual(["runtime-cpu", "det", "rec"])
  })
})
