import type * as Ort from "onnxruntime-web/wasm"

import {
  appScanModels,
  type InstalledScanModels,
  type ScanBackend,
  type ScanModelStore,
} from "../../data"
import { createIndexDots } from "../../domain/scan/dots"
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
import { currentBackend } from "./runtime-choice"

// One recogniser per page load, shared by the single-card and deck screens, so the models are read
// and compiled once. It runs on the ONNX Runtime build the user chose (WebGPU or CPU); the CPU build
// uses one thread, since the site is not cross-origin isolated and worker threads are unavailable.

interface ScanSample {
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
  /** Frees both model sessions once running inferences finish; later calls reject. */
  release(): void
  /** Resolves once both sessions are freed, so the next engine never overlaps with this one. */
  readonly freed: Promise<void>
}

/** A WebGPU session that could not be created; the page offers the CPU build instead. */
export class ScanEngineStartError extends Error {
  readonly backend: ScanBackend

  constructor(backend: ScanBackend, cause: unknown) {
    super(cause instanceof Error ? cause.message : "scan engine failed to start", { cause })
    this.name = "ScanEngineStartError"
    this.backend = backend
  }
}

export type ScanEngineStage = "reading" | "starting"

async function createScanEngine(
  edition: Region,
  models: InstalledScanModels,
  ort: typeof Ort,
  backend: ScanBackend,
): Promise<ScanEngine> {
  ort.env.wasm.wasmBinary = models.runtime
  ort.env.wasm.numThreads = 1
  const options = { executionProviders: [backend === "webgpu" ? "webgpu" : "wasm"] }
  // ONNX Runtime keeps each session's native memory until it is released, not until it is dropped.
  const sessions: Ort.InferenceSession[] = []
  let resolveFreed: () => void = () => undefined
  const freed = new Promise<void>((resolve) => {
    resolveFreed = resolve
  })
  const free = async () => {
    await Promise.allSettled(sessions.map((session) => session.release()))
    resolveFreed()
  }
  let recSession: Ort.InferenceSession
  let detSession: Ort.InferenceSession
  try {
    recSession = await ort.InferenceSession.create(models.rec.bytes, options)
    sessions.push(recSession)
    detSession = await ort.InferenceSession.create(models.det.bytes, options)
    sessions.push(detSession)
    for (const [session, name] of [
      [detSession, "heat"],
      [detSession, "corners"],
      [recSession, "embedding"],
    ] as const)
      if (!session.outputNames.includes(name)) throw new Error(`model output ${name} is missing`)
  } catch (error) {
    await free()
    if (backend === "webgpu") throw new ScanEngineStartError(backend, error)
    throw error
  }
  const life = { running: 0, released: false }
  const isReleased = () => life.released
  const guarded = async <T>(work: () => Promise<T>): Promise<T> => {
    if (life.released) throw new Error("scan engine was released")
    life.running += 1
    try {
      return await work()
    } finally {
      life.running -= 1
      // Read through a call: release() may have run while the work awaited.
      if (isReleased() && life.running === 0) void free()
    }
  }
  const det = models.det.model
  const rec = models.rec.model
  const detBuffer = new Float32Array(3 * det.input * det.input)
  const plane = rec.width * rec.height
  const index: ScanIndex = {
    dim: rec.dim,
    vectors: models.index.vectors,
    scale: models.index.scale,
    hub: models.index.hub,
    cards: models.index.cards,
    scoring: rec.scoring,
    dots: createIndexDots(models.index.vectors, rec.dim),
  }
  let mask: { readonly key: string; readonly rows: Uint8Array | null } = { key: "", rows: null }
  return {
    edition,
    det,
    index,
    cardSize: { width: rec.width, height: rec.height },
    detect: (pixels, threshold) =>
      guarded(async () => {
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
          det.grow,
        )
      }),
    embed: (samples) =>
      guarded(async () => {
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
      }),
    match(views, classes, limit) {
      const key = classes.join(",")
      if (mask.key !== key) mask = { key, rows: classMask(index.cards, classes) }
      return matchCards(index, views, mask.rows, limit)
    },
    confident: (matches) => isConfident(matches, rec.scoring),
    release() {
      if (life.released) return
      life.released = true
      if (life.running === 0) void free()
    },
    freed,
  }
}

let shared: {
  readonly edition: Region
  readonly backend: ScanBackend
  readonly engine: Promise<ScanEngine>
} | null = null
// The engine being replaced; its sessions must be freed before a new one is created.
let retiring: Promise<void> = Promise.resolve()

export interface ScanEngineSources {
  readonly load: (edition: Region, backend: ScanBackend) => Promise<InstalledScanModels>
  readonly runtime: (backend: ScanBackend) => Promise<typeof Ort>
}

/** Models from `store`, ONNX Runtime from the public CDN build this site pins (see data/scan-runtime). */
export function scanEngineSources(store: ScanModelStore = appScanModels): ScanEngineSources {
  return {
    load: (edition, backend) => store.load(edition, backend),
    // Both entries are loaded on demand: the WebGPU build's JavaScript is only fetched when used.
    runtime: async (backend) =>
      backend === "webgpu"
        ? await import("onnxruntime-web/webgpu")
        : await import("onnxruntime-web/wasm"),
  }
}

const appSources = scanEngineSources()

function retire(engine: Promise<ScanEngine>): Promise<void> {
  return engine.then(
    async (built) => {
      built.release()
      await built.freed
    },
    () => undefined,
  )
}

/**
 * The page's engine for an edition and the user's chosen backend; built once from the models on
 * this device. Opening another edition or backend releases the previous engine first.
 */
export function openScanEngine(
  edition: Region,
  onStage: (stage: ScanEngineStage) => void = () => undefined,
  sources: ScanEngineSources = appSources,
  backendOf: () => Promise<ScanBackend> = currentBackend,
): Promise<ScanEngine> {
  return backendOf().then((backend) => {
    if (shared?.edition === edition && shared.backend === backend) return shared.engine
    const before = shared
    if (before) retiring = retire(before.engine)
    const waitFor = retiring
    const engine = (async () => {
      onStage("reading")
      const models = await sources.load(edition, backend)
      onStage("starting")
      await waitFor
      return createScanEngine(edition, models, await sources.runtime(backend), backend)
    })()
    shared = { edition, backend, engine }
    // A failed start must not stick: the next open tries again (for example after a download).
    engine.catch(() => {
      if (shared?.engine === engine) shared = null
    })
    return engine
  })
}

/** Releases the shared engine, e.g. after models are downloaded or removed. */
export function closeScanEngine(): void {
  if (shared) retiring = retire(shared.engine)
  shared = null
}
