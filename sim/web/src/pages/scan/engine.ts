import type * as Ort from "onnxruntime-web/wasm"

import { appScanModels, type InstalledScanModels } from "../../data"
import type { DetModel } from "../../domain/scan/manifest"
import {
  type CardMatch,
  classMask,
  isConfident,
  matchCards,
  type ScanIndex,
} from "../../domain/scan/search"
import { decodeDetections, type Detection, writeSample } from "../../domain/scan/tensor"
import type { Region } from "../../domain/search"

// One recogniser per page load, shared by the single-card and deck screens, so the models are read
// and compiled once. It runs on the CPU build of ONNX Runtime with one thread: the site is not
// cross-origin isolated, so worker threads are unavailable anyway.

export interface ScanSample {
  /** RGBA pixels of a straightened card, `rec.width` × `rec.height`. */
  readonly pixels: Uint8ClampedArray
  /** Also read it turned 180°, for cards facing the other way. */
  readonly turned?: boolean
}

export interface ScanEngine {
  readonly edition: Region
  readonly det: DetModel
  readonly index: ScanIndex
  /** Card size the recogniser reads, in pixels. */
  readonly cardSize: { readonly width: number; readonly height: number }
  /** Finds cards in `det.input` × `det.input` RGBA pixels; quads are in those pixels. */
  detect(pixels: Uint8ClampedArray, threshold: number): Promise<Detection[]>
  /** One embedding per sample (and per turn), in order: [sample0, sample0 turned, sample1, …]. */
  embed(samples: readonly ScanSample[]): Promise<Float32Array[]>
  /** The best `limit` different cards for the views of one crop, within the picked classes. */
  match(views: readonly Float32Array[], classes: readonly string[], limit: number): CardMatch[]
  /** Whether the best match counts as recognised, by the recogniser's own calibration. */
  confident(matches: readonly CardMatch[]): boolean
}

export type ScanEngineStage = "reading" | "starting"

export async function createScanEngine(
  edition: Region,
  models: InstalledScanModels,
  ort: typeof Ort,
): Promise<ScanEngine> {
  ort.env.wasm.wasmBinary = models.runtime
  ort.env.wasm.numThreads = 1
  const options = { executionProviders: ["wasm"] }
  const recSession = await ort.InferenceSession.create(models.rec.bytes, options)
  const detSession = await ort.InferenceSession.create(models.det.bytes, options)
  const det = models.det.model
  const rec = models.rec.model
  const detBuffer = new Float32Array(3 * det.input * det.input)
  const plane = rec.width * rec.height
  for (const [session, name] of [
    [detSession, "heat"],
    [detSession, "corners"],
    [recSession, "embedding"],
  ] as const)
    if (!session.outputNames.includes(name)) throw new Error(`model output ${name} is missing`)
  const index: ScanIndex = {
    dim: rec.dim,
    vectors: models.index.vectors,
    scale: models.index.scale,
    hub: models.index.hub,
    cards: models.index.cards,
    scoring: rec.scoring,
  }
  let mask: { readonly key: string; readonly rows: Uint8Array | null } = { key: "", rows: null }
  return {
    edition,
    det,
    index,
    cardSize: { width: rec.width, height: rec.height },
    async detect(pixels, threshold) {
      writeSample(pixels, detBuffer, 0)
      const out = await detSession.run({
        image: new ort.Tensor("float32", detBuffer, [1, 3, det.input, det.input]),
      })
      const heat = out["heat"]
      const corners = out["corners"]
      if (!heat || !corners) throw new Error("card finder returned no output")
      return decodeDetections(
        heat.data as Float32Array,
        corners.data as Float32Array,
        det.input / det.stride,
        det.stride,
        threshold,
      )
    },
    async embed(samples) {
      const count = samples.reduce((n, sample) => n + (sample.turned ? 2 : 1), 0)
      const batch = new Float32Array(count * 3 * plane)
      let slot = 0
      for (const sample of samples) {
        writeSample(sample.pixels, batch, slot)
        slot += 1
        if (sample.turned) {
          writeSample(sample.pixels, batch, slot, true)
          slot += 1
        }
      }
      const out = await recSession.run({
        image: new ort.Tensor("float32", batch, [count, 3, rec.height, rec.width]),
      })
      const embedding = out["embedding"]
      if (!embedding) throw new Error("recogniser returned no output")
      const data = embedding.data as Float32Array
      return Array.from({ length: count }, (_, at) =>
        data.subarray(at * rec.dim, (at + 1) * rec.dim),
      )
    },
    match(views, classes, limit) {
      const key = classes.join(",")
      if (mask.key !== key) mask = { key, rows: classMask(index.cards, classes) }
      return matchCards(index, views, mask.rows, limit)
    },
    confident: (matches) => isConfident(matches, rec.scoring),
  }
}

let shared: { readonly edition: Region; readonly engine: Promise<ScanEngine> } | null = null

/** The page's engine for an edition; built once from the models on this device. */
export function openScanEngine(
  edition: Region,
  onStage: (stage: ScanEngineStage) => void = () => undefined,
): Promise<ScanEngine> {
  if (shared?.edition === edition) return shared.engine
  const engine = (async () => {
    onStage("reading")
    const models = await appScanModels.load(edition)
    onStage("starting")
    return createScanEngine(edition, models, await import("onnxruntime-web/wasm"))
  })()
  shared = { edition, engine }
  // A failed start must not stick: the next open tries again (for example after a download).
  engine.catch(() => {
    if (shared?.engine === engine) shared = null
  })
  return engine
}

/** Drops the shared engine after models are downloaded or removed. */
export function closeScanEngine(): void {
  shared = null
}
