import {
  type DetModel,
  parseScanManifest,
  type RecModel,
  type ScanFile,
  type ScanIndexFiles,
  type ScanManifest,
  ScanManifestError,
} from "../domain/scan/manifest"
import { parseScanCards, type ScanCard } from "../domain/scan/search"
import type { Region } from "../domain/search"
import { transferDigest } from "./integrity"

// The card scanner's files, downloaded only when the user asks and kept in Cache Storage. Model and
// index files are named by their content hash, so a cached file is valid for as long as it exists;
// each part also keeps a record of what it installed, which keeps scanning working offline and
// across a newer publication until the user updates.

export type ScanPart = "runtime" | "det" | "rec"
export const SCAN_PARTS: readonly ScanPart[] = ["runtime", "det", "rec"]

export type ScanModelErrorKind = "network" | "integrity" | "quota" | "unsupported" | "missing"

export class ScanModelError extends Error {
  readonly kind: ScanModelErrorKind

  constructor(kind: ScanModelErrorKind, message: string) {
    super(message)
    this.name = "ScanModelError"
    this.kind = kind
  }
}

export interface ScanPartStatus {
  /** `update`: an older installed copy still works, and a newer one is published. */
  readonly state: "ready" | "update" | "missing"
  /** Bytes the part takes on the device once downloaded (the published copy when known). */
  readonly bytes: number
  /** Bytes still to download. */
  readonly missingBytes: number
  /** Model version shown as `det-N`／`rec-N`; the runtime has none. */
  readonly version?: number
  /** Card data version of the recogniser's index for this edition. */
  readonly dataVersion?: string
  /** Cards in that index. */
  readonly cards?: number
  /** Bytes of that index, included in `bytes`; the rest is the model. */
  readonly indexBytes?: number
}

export interface ScanModelsStatus {
  /** `offline`: the list could not be read, so only installed parts are known. */
  readonly manifest: "ok" | "offline" | "unsupported" | "invalid"
  readonly parts: Readonly<Record<ScanPart, ScanPartStatus>>
  readonly storedBytes: number
}

export interface InstalledScanModels {
  readonly runtime: Uint8Array
  readonly det: { readonly model: DetModel; readonly bytes: Uint8Array }
  readonly rec: { readonly model: RecModel; readonly bytes: Uint8Array }
  readonly index: {
    readonly files: ScanIndexFiles
    readonly vectors: Int8Array
    readonly scale: Float32Array
    readonly hub: Float32Array
    readonly cards: readonly ScanCard[]
  }
}

interface RuntimeRecord {
  readonly url: string
  readonly bytes: number
}
interface RecRecord {
  readonly rec: RecModel
  readonly index: Partial<Record<Region, ScanIndexFiles>>
}
interface Records {
  runtime?: RuntimeRecord
  det?: DetModel
  rec?: RecRecord
}

export interface ScanModelStoreOptions {
  /** Read at call time: Cache Storage is missing outside secure contexts. */
  readonly caches: () => CacheStorage
  readonly fetcher: (url: string, init?: RequestInit) => Promise<Response>
  /** Root URL of the published scan files (`…/scan`), read at call time. */
  readonly scanBase: () => string
  /** The ONNX Runtime binary this build ships as a site asset. */
  readonly runtime: RuntimeRecord
}

export type ScanProgress = (got: number, total: number) => void

const CACHE = "sve-scan-models-v1"
// Cache keys only identify entries; files are fetched from the scan root of the active snapshot.
const KEY = "/scan-models/"
const recordKey = (part: ScanPart) => `${KEY}installed/${part}.json`
const fileKey = (file: string) => `${KEY}${file}`
const runtimeKey = (url: string) => `${KEY}runtime/${encodeURIComponent(url)}`
const SIZE_HEADER = "x-scan-bytes"

function isQuotaError(error: unknown): boolean {
  return error instanceof DOMException && error.name === "QuotaExceededError"
}

function indexList(index: ScanIndexFiles): ScanFile[] {
  return [index.vectors, index.scale, index.hub, index.cards]
}

function sum(files: readonly { readonly bytes: number }[]): number {
  return files.reduce((total, file) => total + file.bytes, 0)
}

export function createScanModelStore(options: ScanModelStoreOptions) {
  const open = () => options.caches().open(CACHE)

  async function records(cache: Cache): Promise<Records> {
    const out: Records = {}
    for (const part of SCAN_PARTS) {
      const hit = await cache.match(recordKey(part))
      if (!hit) continue
      try {
        Object.assign(out, { [part]: JSON.parse(await hit.text()) as unknown })
      } catch {
        await cache.delete(recordKey(part))
      }
    }
    return out
  }

  async function has(cache: Cache, key: string): Promise<boolean> {
    return (await cache.match(key)) !== undefined
  }

  async function missing(cache: Cache, files: readonly ScanFile[]): Promise<number> {
    let bytes = 0
    for (const file of files) if (!(await has(cache, fileKey(file.file)))) bytes += file.bytes
    return bytes
  }

  async function manifest(): Promise<
    | { readonly state: "ok"; readonly manifest: ScanManifest }
    | { readonly state: "offline" | "unsupported" | "invalid" }
  > {
    let response: Response
    try {
      response = await options.fetcher(`${options.scanBase()}/manifest.json`, { cache: "no-cache" })
    } catch {
      return { state: "offline" }
    }
    if (!response.ok) return { state: "offline" }
    try {
      return { state: "ok", manifest: parseScanManifest(await response.json()) }
    } catch (error) {
      return {
        state:
          error instanceof ScanManifestError && error.kind === "unsupported"
            ? "unsupported"
            : "invalid",
      }
    }
  }

  async function storedBytes(cache: Cache): Promise<number> {
    let total = 0
    for (const request of await cache.keys()) {
      const response = await cache.match(request)
      total += Number(response?.headers.get(SIZE_HEADER) ?? 0)
    }
    return total
  }

  async function status(edition: Region): Promise<ScanModelsStatus> {
    const cache = await open()
    const [listed, installed] = await Promise.all([manifest(), records(cache)])
    const published = listed.state === "ok" ? listed.manifest : undefined

    const runtimeReady = await has(cache, runtimeKey(options.runtime.url))
    const runtime: ScanPartStatus = {
      state: runtimeReady ? "ready" : "missing",
      bytes: options.runtime.bytes,
      missingBytes: runtimeReady ? 0 : options.runtime.bytes,
    }

    interface Copy {
      readonly files: ScanFile[]
      readonly version: number
      readonly index?: ScanIndexFiles
    }
    const describe = (copy: Copy) => ({
      bytes: sum(copy.files),
      version: copy.version,
      ...(copy.index
        ? {
            dataVersion: copy.index.dataVersion,
            cards: copy.index.count,
            indexBytes: sum(indexList(copy.index)),
          }
        : {}),
    })
    const part = async (
      want: Copy | undefined,
      have: Copy | undefined,
    ): Promise<ScanPartStatus> => {
      const haveReady = have !== undefined && (await missing(cache, have.files)) === 0
      if (want) {
        const lacking = await missing(cache, want.files)
        // Scanning reads the installed record, so the files alone do not make the published copy ready.
        const current =
          have !== undefined &&
          have.files.length === want.files.length &&
          have.files.every((file, at) => file.sha256 === want.files[at]?.sha256)
        const state = lacking === 0 && current ? "ready" : haveReady ? "update" : "missing"
        return { state, missingBytes: lacking, ...describe(want) }
      }
      if (have && haveReady) return { state: "ready", missingBytes: 0, ...describe(have) }
      return { state: "missing", bytes: 0, missingBytes: 0 }
    }

    const det = await part(
      published && { files: [published.det], version: published.det.version },
      installed.det && { files: [installed.det], version: installed.det.version },
    )
    const wantIndex = published?.index[edition]
    const haveIndex = installed.rec?.index[edition]
    const rec = await part(
      published &&
        wantIndex && {
          files: [published.rec, ...indexList(wantIndex)],
          version: published.rec.version,
          index: wantIndex,
        },
      installed.rec &&
        haveIndex && {
          files: [installed.rec.rec, ...indexList(haveIndex)],
          version: installed.rec.rec.version,
          index: haveIndex,
        },
    )
    return {
      manifest: listed.state,
      parts: { runtime, det, rec },
      storedBytes: await storedBytes(cache),
    }
  }

  async function fetchFile(
    url: string,
    expected: { readonly bytes: number; readonly sha256?: string },
    onBytes: (got: number) => void,
    signal: AbortSignal | undefined,
  ): Promise<Uint8Array> {
    let response: Response
    try {
      response = await options.fetcher(url, signal ? { signal } : {})
    } catch (error) {
      if (signal?.aborted) throw error
      throw new ScanModelError("network", error instanceof Error ? error.message : "fetch failed")
    }
    if (!response.ok)
      throw new ScanModelError("network", `HTTP ${String(response.status)} for ${url}`)
    const chunks: Uint8Array[] = []
    let got = 0
    try {
      if (response.body) {
        const reader = response.body.getReader()
        for (;;) {
          const chunk = await reader.read()
          if (chunk.done) break
          chunks.push(chunk.value)
          got += chunk.value.length
          onBytes(got)
        }
      } else {
        const whole = new Uint8Array(await response.arrayBuffer())
        chunks.push(whole)
        got = whole.length
        onBytes(got)
      }
    } catch (error) {
      if (signal?.aborted) throw error
      throw new ScanModelError("network", error instanceof Error ? error.message : "body failed")
    }
    const bytes = new Uint8Array(got)
    let offset = 0
    for (const chunk of chunks) {
      bytes.set(chunk, offset)
      offset += chunk.length
    }
    if (bytes.length !== expected.bytes)
      throw new ScanModelError(
        "integrity",
        `${url}: ${String(bytes.length)} bytes, expected ${String(expected.bytes)}`,
      )
    if (
      expected.sha256 !== undefined &&
      (await transferDigest(bytes)) !== `sha256:${expected.sha256}`
    )
      throw new ScanModelError("integrity", `${url}: SHA-256 does not match`)
    return bytes
  }

  async function put(cache: Cache, key: string, body: Uint8Array | string): Promise<void> {
    const bytes = typeof body === "string" ? new TextEncoder().encode(body) : body
    try {
      await cache.put(
        key,
        // Install records are not model data, so they do not count toward the stored size.
        new Response(
          bytes as Uint8Array<ArrayBuffer>,
          typeof body === "string" ? {} : { headers: { [SIZE_HEADER]: String(bytes.length) } },
        ),
      )
    } catch (error) {
      if (isQuotaError(error)) throw new ScanModelError("quota", "device storage is full")
      throw error
    }
  }

  /** Cache keys some install record still needs. */
  function referenced(installed: Records): Set<string> {
    const keys = new Set<string>()
    if (installed.runtime) keys.add(runtimeKey(installed.runtime.url))
    if (installed.det) keys.add(fileKey(installed.det.file))
    if (installed.rec)
      for (const file of [
        installed.rec.rec,
        ...Object.values(installed.rec.index).flatMap((entry) => indexList(entry)),
      ])
        keys.add(fileKey(file.file))
    return keys
  }

  /** Downloads whatever the part lacks, verifies it, then switches the part over and drops its old files. */
  async function download(
    part: ScanPart,
    edition: Region,
    onProgress: ScanProgress,
    signal?: AbortSignal,
  ): Promise<void> {
    const cache = await open()
    // Files this attempt stored; a failure removes the ones no install record uses, so nothing is
    // left behind that the delete buttons cannot reach.
    const added: string[] = []
    const fetchInto = async (
      key: string,
      url: string,
      file: { bytes: number; sha256?: string },
      onBytes: (got: number) => void,
    ) => {
      if (await has(cache, key)) return
      const body = await fetchFile(url, file, onBytes, signal)
      added.push(key)
      await put(cache, key, body)
    }
    try {
      if (part === "runtime") {
        const { url, bytes } = options.runtime
        onProgress(0, bytes)
        await fetchInto(runtimeKey(url), url, { bytes }, (got) => {
          onProgress(got, bytes)
        })
        onProgress(bytes, bytes)
        await put(cache, recordKey("runtime"), JSON.stringify(options.runtime))
        for (const request of await cache.keys()) {
          const path = new URL(request.url).pathname
          if (path.startsWith(`${KEY}runtime/`) && path !== runtimeKey(url))
            await cache.delete(request)
        }
        return
      }
      const listed = await manifest()
      if (listed.state === "unsupported")
        throw new ScanModelError("unsupported", "the scan list needs a newer site")
      if (listed.state !== "ok") throw new ScanModelError("network", "the scan list is unavailable")
      const published = listed.manifest
      const index = published.index[edition]
      if (part === "rec" && !index)
        throw new ScanModelError("missing", `no ${edition} card index is published`)
      const files =
        part === "det" ? [published.det] : [published.rec, ...indexList(index as ScanIndexFiles)]
      const total = sum(files)
      let done = 0
      onProgress(0, total)
      for (const file of files) {
        await fetchInto(fileKey(file.file), `${options.scanBase()}/${file.file}`, file, (got) => {
          onProgress(done + got, total)
        })
        done += file.bytes
        onProgress(done, total)
      }
      const installed = await records(cache)
      const before = referenced(installed)
      if (part === "det") await put(cache, recordKey("det"), JSON.stringify(published.det))
      else {
        // Another edition's index stays only while it was built for the same recogniser.
        const previous =
          installed.rec?.rec.sha256 === published.rec.sha256 ? installed.rec.index : {}
        const record: RecRecord = { rec: published.rec, index: { ...previous, [edition]: index } }
        await put(cache, recordKey("rec"), JSON.stringify(record))
      }
      const after = referenced(await records(cache))
      for (const key of before) if (!after.has(key)) await cache.delete(key)
    } catch (error) {
      const keep = referenced(await records(cache))
      for (const key of added) if (!keep.has(key)) await cache.delete(key)
      throw error
    }
  }

  async function removeFiles(cache: Cache, part: ScanPart, installed: Records): Promise<void> {
    if (part === "runtime") {
      for (const request of await cache.keys())
        if (new URL(request.url).pathname.startsWith(`${KEY}runtime/`)) await cache.delete(request)
    } else if (part === "det") {
      if (installed.det) await cache.delete(fileKey(installed.det.file))
    } else if (installed.rec) {
      const files = [
        installed.rec.rec,
        ...Object.values(installed.rec.index).flatMap((entry) => indexList(entry)),
      ]
      for (const file of files) await cache.delete(fileKey(file.file))
    }
    await cache.delete(recordKey(part))
  }

  async function remove(part: ScanPart): Promise<void> {
    const cache = await open()
    await removeFiles(cache, part, await records(cache))
  }

  async function removeAll(): Promise<void> {
    await options.caches().delete(CACHE)
  }

  async function read(cache: Cache, key: string): Promise<Uint8Array> {
    const hit = await cache.match(key)
    if (!hit) throw new ScanModelError("missing", `${key} is not on this device`)
    return new Uint8Array(await hit.arrayBuffer())
  }

  /** The installed models for one card edition, read from the device only. */
  async function load(edition: Region): Promise<InstalledScanModels> {
    const cache = await open()
    const installed = await records(cache)
    const index = installed.rec?.index[edition]
    // The runtime binary must come from the same ONNX Runtime build as this site's JavaScript.
    if (
      installed.runtime?.url !== options.runtime.url ||
      !installed.det ||
      !installed.rec ||
      !index
    )
      throw new ScanModelError("missing", "scan models are not downloaded")
    const [runtime, det, rec, vectors, scale, hub, cards] = await Promise.all([
      read(cache, runtimeKey(options.runtime.url)),
      read(cache, fileKey(installed.det.file)),
      read(cache, fileKey(installed.rec.rec.file)),
      read(cache, fileKey(index.vectors.file)),
      read(cache, fileKey(index.scale.file)),
      read(cache, fileKey(index.hub.file)),
      read(cache, fileKey(index.cards.file)),
    ])
    // Copies keep the typed views aligned whatever offset the cache hands back.
    return {
      runtime,
      det: { model: installed.det, bytes: det },
      rec: { model: installed.rec.rec, bytes: rec },
      index: {
        files: index,
        vectors: new Int8Array(vectors.slice().buffer),
        scale: new Float32Array(scale.slice().buffer),
        hub: new Float32Array(hub.slice().buffer),
        cards: parseScanCards(JSON.parse(new TextDecoder().decode(cards)), index.count),
      },
    }
  }

  return { status, download, remove, removeAll, load }
}

export type ScanModelStore = ReturnType<typeof createScanModelStore>
