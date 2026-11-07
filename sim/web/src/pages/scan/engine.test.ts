import type * as Ort from "onnxruntime-web/wasm"
import { describe, expect, it } from "vitest"

import { createScanModelStore, SCAN_PARTS } from "../../data/scan-models"
import { memoryCache } from "../../test-utils/cache"
import { fakeScanCdn, RUNTIME, SCAN_BASE } from "../../test-utils/scan-publication"
import { createScanEngine } from "./engine"

interface Run {
  readonly model: string
  readonly dims: readonly number[]
  readonly data: Float32Array
}

/** Stands in for ONNX Runtime: records each run and answers like the real models' output names. */
function fakeOrt(runs: Run[]) {
  class Tensor {
    readonly data: Float32Array
    readonly dims: readonly number[]
    constructor(_type: string, data: Float32Array, dims: readonly number[]) {
      this.data = data
      this.dims = dims
    }
  }
  const session = (model: string) => ({
    outputNames: model.startsWith("det") ? ["heat", "corners"] : ["embedding"],
    run: (feeds: { image: Tensor }) => {
      const { data, dims } = feeds.image
      runs.push({ model, dims, data: data.slice() })
      if (model.startsWith("det")) {
        const heat = new Float32Array(32 * 32)
        heat[33] = 0.9
        return Promise.resolve({
          heat: { data: heat },
          corners: { data: new Float32Array(8 * 32 * 32) },
        })
      }
      // Each sample's embedding is its first and last red value, so order and turns are visible.
      const n = dims[0] ?? 0
      const plane = (dims[2] ?? 0) * (dims[3] ?? 0)
      const out = new Float32Array(n * 4)
      for (let i = 0; i < n; i += 1) {
        out[i * 4] = data[i * 3 * plane] ?? 0
        out[i * 4 + 1] = data[i * 3 * plane + plane - 1] ?? 0
      }
      return Promise.resolve({ embedding: { data: out } })
    },
  })
  const ort = {
    env: { wasm: {} as Record<string, unknown> },
    Tensor,
    InferenceSession: {
      create: (bytes: Uint8Array) => Promise.resolve(session(new TextDecoder().decode(bytes))),
    },
  }
  return ort
}

async function installedModels() {
  const cdn = fakeScanCdn()
  const storage = memoryCache()
  const store = createScanModelStore({
    caches: () => storage,
    fetcher: cdn.fetcher,
    scanBase: () => SCAN_BASE,
    runtime: RUNTIME,
  })
  for (const part of SCAN_PARTS) await store.download(part, "jp", () => undefined)
  return store.load("jp")
}

describe("scan engine", () => {
  it("hands the stored runtime to ONNX Runtime and reads the models' conventions from the manifest", async () => {
    const runs: Run[] = []
    const ort = fakeOrt(runs)
    const engine = await createScanEngine(
      "jp",
      await installedModels(),
      ort as unknown as typeof Ort,
    )
    expect((ort.env.wasm["wasmBinary"] as Uint8Array).length).toBe(RUNTIME.bytes)
    expect(ort.env.wasm["numThreads"]).toBe(1)
    expect(engine.cardSize).toEqual({ width: 224, height: 312 })
    expect(engine.det.grow).toBe(1.6)
    expect(engine.index.scoring.accept).toBe(0.5)
    expect(engine.index.cards).toHaveLength(5)
  })

  it("finds cards on the finder's square input", async () => {
    const runs: Run[] = []
    const engine = await createScanEngine(
      "jp",
      await installedModels(),
      fakeOrt(runs) as unknown as typeof Ort,
    )
    const found = await engine.detect(new Uint8ClampedArray(256 * 256 * 4), 0.5)
    expect(runs[0]?.dims).toEqual([1, 3, 256, 256])
    expect(found).toHaveLength(1)
    expect(found[0]?.quad[0]).toEqual([12, 12])
  })

  it("embeds every sample, and its 180° turn when asked, in one batch and in order", async () => {
    const runs: Run[] = []
    const engine = await createScanEngine(
      "jp",
      await installedModels(),
      fakeOrt(runs) as unknown as typeof Ort,
    )
    const card = (first: number, last: number) => {
      const pixels = new Uint8ClampedArray(224 * 312 * 4)
      pixels[0] = first
      pixels[pixels.length - 4] = last
      return pixels
    }
    const out = await engine.embed([
      { pixels: card(255, 0), turned: true },
      { pixels: card(0, 255) },
    ])
    expect(runs[0]?.dims).toEqual([3, 3, 312, 224])
    expect(out.map((row) => Array.from(row.subarray(0, 2)))).toEqual([
      [1, 0],
      [0, 1],
      [0, 1],
    ])
  })

  it("matches within the picked classes and judges the best card by the recogniser's calibration", async () => {
    const engine = await createScanEngine(
      "jp",
      await installedModels(),
      fakeOrt([]) as unknown as typeof Ort,
    )
    const elfLike = Float32Array.from([0.9, 0.3, 0, 0])
    const all = engine.match([elfLike], [], 3)
    expect(all.map((match) => match.card.no)).toEqual(["BP01-001", "BP01-002", "BP01-003"])
    expect(engine.confident(all)).toBe(true)
    const dragons = engine.match([elfLike], ["dragon"], 3)
    expect(dragons.map((match) => match.card.no)).toEqual(["BP01-002", "BP01-T01"])
    expect(engine.confident(dragons)).toBe(false)
  })
})
