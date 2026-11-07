import { describe, expect, it } from "vitest"

import { scanRuntimeBinaries } from "./scan-runtime"

const pins = JSON.stringify({
  version: "1.31.1",
  cpu: { file: "ort-wasm-simd-threaded.wasm", bytes: 14848852, sha256: "a".repeat(64) },
  webgpu: { file: "ort-wasm-simd-threaded.asyncify.wasm", bytes: 28747738, sha256: "b".repeat(64) },
})

describe("scan runtime pins", () => {
  it("reads each backend's binary with its build-time size and hash", () => {
    const binaries = scanRuntimeBinaries(pins)
    expect(binaries.cpu).toMatchObject({ bytes: 14848852, sha256: "a".repeat(64) })
    expect(binaries.webgpu).toMatchObject({ bytes: 28747738, sha256: "b".repeat(64) })
  })

  it("names the pinned version on jsDelivr first and unpkg second", () => {
    expect(scanRuntimeBinaries(pins).webgpu.urls).toEqual([
      "https://cdn.jsdelivr.net/npm/onnxruntime-web@1.31.1/dist/ort-wasm-simd-threaded.asyncify.wasm",
      "https://unpkg.com/onnxruntime-web@1.31.1/dist/ort-wasm-simd-threaded.asyncify.wasm",
    ])
  })
})
