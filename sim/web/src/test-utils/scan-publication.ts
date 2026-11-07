import { createHash } from "node:crypto"

import type { ScanBackend, ScanRuntimeBinary } from "../data/scan-runtime"
import { memoryCache } from "./cache"

// A tiny synthetic scan publication: placeholder model bytes (never a real model), a 4-dimensional
// card index and a manifest whose sizes and hashes match, served from an in-memory "CDN".

export const SCAN_BASE = "https://cdn.test/scan"
const hex = (bytes: Uint8Array) => createHash("sha256").update(bytes).digest("hex")

// Stand-ins for the two ONNX Runtime binaries: same pinned hashes and CDN addresses as the build.
const RUNTIME_BYTES: Record<ScanBackend, Uint8Array> = {
  cpu: new Uint8Array(64).fill(3),
  webgpu: new Uint8Array(96).fill(5),
}
const RUNTIME_FILES: Record<ScanBackend, string> = {
  cpu: "ort-wasm-simd-threaded.wasm",
  webgpu: "ort-wasm-simd-threaded.jsep.wasm",
}
const VERSION = "1.31.1"
export const RUNTIMES: Readonly<Record<ScanBackend, ScanRuntimeBinary>> = {
  cpu: runtimeBinary("cpu"),
  webgpu: runtimeBinary("webgpu"),
}
function runtimeBinary(backend: ScanBackend): ScanRuntimeBinary {
  const file = RUNTIME_FILES[backend]
  return {
    file,
    bytes: RUNTIME_BYTES[backend].length,
    sha256: hex(RUNTIME_BYTES[backend]),
    urls: [
      `https://cdn.jsdelivr.net/npm/onnxruntime-web@${VERSION}/dist/${file}`,
      `https://unpkg.com/onnxruntime-web@${VERSION}/dist/${file}`,
    ],
  }
}

export interface FakeCard {
  readonly no: string
  readonly c: string
  readonly a?: string
  readonly k?: string
  /** Index vector, 4 int8 values. */
  readonly v: readonly number[]
  readonly hub?: number
}

export interface PublicationOptions {
  readonly det?: number
  readonly rec?: number
  readonly jp?: readonly FakeCard[]
  readonly en?: readonly FakeCard[] | null
  readonly dataVersion?: string
  readonly format?: number
}

export const DEFAULT_CARDS: readonly FakeCard[] = [
  { no: "BP01-001", c: "c1", k: "elf", v: [127, 0, 0, 0] },
  { no: "BP01-001SL", c: "c1", k: "elf", v: [120, 10, 0, 0] },
  { no: "BP01-002", c: "c2", k: "dragon", v: [0, 127, 0, 0] },
  { no: "BP01-003", c: "c3", k: "neutral", v: [0, 0, 127, 0] },
  { no: "BP01-T01", c: "c4", k: "", v: [0, 0, 0, 127], hub: 0.5 },
]

/** A publication's manifest and files; `files` maps scan-root paths to bytes. */
export function scanPublication(options: PublicationOptions = {}) {
  const files = new Map<string, Uint8Array>()
  const add = (dir: "models" | "index", ext: string, bytes: Uint8Array) => {
    const sha256 = hex(bytes)
    const file = `${dir}/${sha256}.${ext}`
    files.set(file, bytes)
    return { file, bytes: bytes.length, sha256 }
  }
  const det = options.det ?? 1
  const rec = options.rec ?? 1
  const index = (cards: readonly FakeCard[], region: string) => {
    const vectors = new Int8Array(cards.flatMap((card) => card.v))
    const scale = new Float32Array(cards.map(() => 1 / 127))
    const hub = new Float32Array(cards.map((card) => card.hub ?? 0))
    const list = cards.map((card, at) => ({
      no: card.no,
      c: card.c,
      n: 100 + at,
      f: 0,
      nm: `${region} ${card.no}`,
      a: card.a ?? "a0",
      k: card.k ?? "",
      y: "follower",
    }))
    return {
      rec,
      data_version: options.dataVersion ?? "2026-09-29",
      count: cards.length,
      vectors: add("index", "i8", new Uint8Array(vectors.buffer)),
      scale: add("index", "scale", new Uint8Array(scale.buffer)),
      hub: add("index", "hub", new Uint8Array(hub.buffer)),
      cards: add("index", "json", new TextEncoder().encode(JSON.stringify(list))),
    }
  }
  const manifest = {
    format: options.format ?? 1,
    det: {
      version: det,
      ...add("models", "onnx", new TextEncoder().encode(`det model ${String(det)}`)),
      input: 256,
      stride: 8,
      grow: 1.0,
    },
    rec: {
      version: rec,
      ...add("models", "onnx", new TextEncoder().encode(`rec model ${String(rec)}`)),
      input: [312, 224],
      dim: 4,
      scoring: { hubWeight: 0.75, accept: 0.5, margin: 0.03 },
    },
    index: {
      jp: index(options.jp ?? DEFAULT_CARDS, "jp"),
      ...(options.en === null ? {} : { en: index(options.en ?? DEFAULT_CARDS, "en") }),
    },
  }
  return { manifest, files }
}

export type ScanPublication = ReturnType<typeof scanPublication>

/** An in-memory CDN for one publication at a time, with switches for the failures under test. */
export function fakeScanCdn(initial: ScanPublication | null = scanPublication()) {
  let publication = initial
  const requests: string[] = []
  const corrupt = new Set<string>()
  let offline = false
  // Runtime URLs (by CDN prefix) that cannot be reached, and runtime files served with wrong bytes.
  const unreachable = new Set<string>()
  const runtimeCorrupt = new Set<ScanBackend>()
  const fetcher = (url: string): Promise<Response> => {
    requests.push(url)
    if (offline) return Promise.reject(new TypeError("Failed to fetch"))
    const runtime = (Object.keys(RUNTIMES) as ScanBackend[]).find((b) =>
      RUNTIMES[b].urls.includes(url),
    )
    if (runtime) {
      if (unreachable.has(url)) return Promise.reject(new TypeError("Failed to fetch"))
      const bytes = RUNTIME_BYTES[runtime]
      return Promise.resolve(
        new Response(
          (runtimeCorrupt.has(runtime)
            ? bytes.map((byte) => byte ^ 1)
            : bytes) as Uint8Array<ArrayBuffer>,
        ),
      )
    }
    if (!publication || !url.startsWith(`${SCAN_BASE}/`))
      return Promise.resolve(new Response(null, { status: 404 }))
    const path = url.slice(SCAN_BASE.length + 1)
    if (path === "manifest.json") return Promise.resolve(Response.json(publication.manifest))
    const bytes = publication.files.get(path)
    if (!bytes) return Promise.resolve(new Response(null, { status: 404 }))
    const body = corrupt.has(path) ? bytes.map((byte) => byte ^ 1) : bytes
    return Promise.resolve(new Response(body as Uint8Array<ArrayBuffer>))
  }
  return {
    fetcher,
    requests,
    publish(next: ScanPublication | null) {
      publication = next
    },
    corrupt(file: string) {
      corrupt.add(file)
    },
    setOffline(value: boolean) {
      offline = value
    },
    /** Makes one runtime CDN address unreachable, e.g. the first CDN being down. */
    unreachable(url: string) {
      unreachable.add(url)
    },
    corruptRuntime(backend: ScanBackend) {
      runtimeCorrupt.add(backend)
    },
  }
}

/** Cache Storage whose writes fail as a full device does. */
export function fullCache(): CacheStorage {
  const inner = memoryCache()
  return {
    keys: () => inner.keys(),
    delete: (name: string) => inner.delete(name),
    open: async (name: string) => {
      const cache = await inner.open(name)
      return {
        match: (key: string) => cache.match(key),
        keys: () => cache.keys(),
        delete: (key: string) => cache.delete(key),
        put: () => Promise.reject(new DOMException("full", "QuotaExceededError")),
      } as unknown as Cache
    },
  } as Partial<CacheStorage> as CacheStorage
}
