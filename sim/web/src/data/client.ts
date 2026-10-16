import { catalogOf } from "./catalog"
import { fetchBytes, type Fetcher, NetworkError } from "./cdn"
import { createDecoder } from "./format-v3/decoder"
import { SnapshotError } from "./format-v3/errors"
import {
  arrayValue,
  canonicalText,
  integerValue,
  type JsonObject,
  type JsonValue,
  objectValue,
  parseJson,
  stringValue,
} from "./format-v3/json"
import {
  catalogKeys,
  type Files,
  findBase,
  type Fragment,
  isCompatible,
  joinDetail,
  validateCatalog,
} from "./format-v3/reader"
import { checkEntry, indexEntries } from "./index-entry"
import { transferDigest } from "./integrity"
import { MetadataBytes, type MetadataProgress } from "./metadata"
import { appRequests, requestQueue } from "./request-queue"
import { VerifiedCache } from "./verified-cache"

type LoadPhase = "index" | "manifest" | "config" | "bootstrap" | "search" | "images"
type ErrorKind = "network" | "incompatible" | "corrupt"
interface LoadFailure {
  readonly kind: ErrorKind
  readonly detail: string
}
export type SnapshotStatus =
  | { readonly state: "idle" }
  /** During the bootstrap phase of a first download, `done` of `total` startup files have arrived. */
  | {
      readonly state: "loading"
      readonly phase: LoadPhase
      readonly done?: number
      readonly total?: number
    }
  /**
   * `updating` is the phase of a reload while the previous snapshot stays active; `updateError`
   * is set when such a reload failed.
   */
  | {
      readonly state: "ready"
      readonly dataVersion: string
      readonly updating?: LoadPhase
      readonly updateError?: LoadFailure
      readonly outdated?: boolean
      /** Local verified data has not yet been confirmed by the server this session. */
      readonly unconfirmed?: boolean
    }
  | ({ readonly state: "error" } & LoadFailure)

export interface LoadedSnapshot {
  readonly dataVersion: string
  readonly region?: "jp" | "en"
  readonly printingReady?: boolean
  readonly entrySource?: "current" | "previous"
  readonly manifestHash: string
  readonly manifest: JsonObject
  readonly files: Files
  readonly config: JsonObject
  /** Every bootstrap fragment, decoded; detail files are joined onto these on demand. */
  readonly bootstrap: readonly Fragment[]
}

export interface SnapshotClientOptions {
  readonly region?: "jp" | "en"
  readonly fetch?: Fetcher
  /** `index`: the current/previous version index (format §4.1); `preview`: a preview root's pointer file. */
  readonly entry?: "index" | "preview"
  readonly detailCacheSize?: number
  readonly cacheStorage?: CacheStorage
}

export interface SnapshotClient {
  readonly base: string
  readonly status: () => SnapshotStatus
  readonly subscribe: (listener: () => void) => () => void
  /** Resolves when ready (including local data); server confirmation may still be running. */
  readonly load: () => Promise<void>
  readonly retry: () => Promise<void>
  /** Reads the version index again even when ready; the active snapshot only changes on success. */
  readonly reload: () => Promise<void>
  readonly setRegion?: (region: "jp" | "en") => void
  readonly selectRegion?: (region: "jp" | "en") => Promise<void>
  readonly snapshot: () => LoadedSnapshot | null
  /** Loads another catalog without changing the active region or its readiness. */
  readonly catalog?: (region: "jp" | "en", signal: AbortSignal) => Promise<LoadedSnapshot>
  /** A text, history or images file, decoded and joined onto its bootstrap base; cached LRU. */
  readonly fragments: (
    fileKey: string,
    backgroundSignal?: AbortSignal,
  ) => Promise<readonly Fragment[]>
  readonly metadataStatus: () => MetadataProgress
  readonly prefetchPrintings?: () => Promise<void>
  readonly prefetchImages: () => Promise<void>
  readonly cancelImagePrefetch: () => void
}

const INDEX_PATH = "snapshots/versions/index.json"
const INDEX_FORMAT = 2
// Preview roots have no permanent index (format §4.2); until #4 Q12 settles, dev reads this pointer.
const PREVIEW_POINTER = "snapshots/preview/current.json"
// Bump whenever the stored snapshot record changes shape.
const SNAPSHOT_RECORD_FORMAT = 2
class NoCompatibleVersion extends Error {}

function failureOf(error: unknown): LoadFailure {
  if (error instanceof NetworkError) return { kind: "network", detail: error.message }
  if (error instanceof NoCompatibleVersion) return { kind: "incompatible", detail: error.message }
  if (error instanceof SnapshotError) {
    return error.code === "unsupported-version"
      ? { kind: "incompatible", detail: error.message }
      : { kind: "corrupt", detail: error.message }
  }
  return { kind: "corrupt", detail: error instanceof Error ? error.message : String(error) }
}

export function createSnapshotClient(
  base: string,
  options: SnapshotClientOptions = {},
): SnapshotClient {
  const requests = options.fetch ? requestQueue(options.fetch) : appRequests
  const fetcher = requests.foreground
  const entry = options.entry ?? "index"
  const decode = createDecoder()
  const stored = new VerifiedCache(
    base,
    "snapshot",
    SNAPSHOT_RECORD_FORMAT,
    options.cacheStorage ?? globalThis.caches,
  )
  const cacheSize = options.detailCacheSize ?? 64
  let metadata: MetadataBytes | undefined
  let detailBytes = 0
  const listeners = new Set<() => void>()
  let status: SnapshotStatus = { state: "idle" }
  let region = options.region ?? "jp"
  let loaded: LoadedSnapshot | null = null
  let pending: Promise<void> | null = null
  let startup: Promise<void> | null = null
  let reloadRequested = false
  let lastRevision = -1
  let lastIndex = ""
  let detailAbort = new AbortController()
  // Settled detail files in LRU order; requests still in flight live apart so eviction never
  // drops a promise other callers are waiting on.
  let details = new Map<string, readonly Fragment[]>()
  let inflight = new Map<string, Promise<readonly Fragment[]>>()
  const catalogs = new Map<string, LoadedSnapshot>()
  // Comparison catalogs are complete startup sets that no record holds yet; the first adoption
  // stores one, so the next visit restores it instead of downloading again.
  const unsaved = new WeakSet<LoadedSnapshot>()

  const set = (next: SnapshotStatus) => {
    status = next
    for (const listener of listeners) listener()
  }
  const url = (path: string) => `${base}/${path}`

  /** Exact bytes or nothing: one retry covers a torn download, a second mismatch is corruption. */
  const fetchVerified = async (
    path: string,
    sha256: string,
    bytes?: number,
    signal?: AbortSignal,
    priority: "foreground" | "background" | "low" = "foreground",
  ): Promise<Uint8Array> => {
    for (let attempt = 0; ; attempt += 1) {
      const data = await fetchBytes(
        priority === "foreground" ? fetcher : requests.background,
        url(path),
        {
          ...(signal ? { signal } : {}),
          ...(priority === "low" ? { priority: "low" } : {}),
        },
      )
      if ((await transferDigest(data)) === sha256 && (bytes === undefined || data.length === bytes))
        return data
      if (attempt === 1)
        throw new SnapshotError("blob-integrity", `hash mismatch for ${path} after retry`)
    }
  }

  // Index formats are independent of the snapshot format and need their own admission check.
  const requireIndexFormat = (value: JsonObject, what: string): void => {
    const format = value["index_format"]
    if (typeof format === "number" && Number.isInteger(format) && format > INDEX_FORMAT) {
      throw new NoCompatibleVersion(
        `${what} uses an index format this version of the site cannot read`,
      )
    }
    if (format !== INDEX_FORMAT)
      throw new SnapshotError("shape", `${what} has an invalid index format`)
  }

  const requestEntry = (): Promise<Uint8Array> =>
    fetchBytes(fetcher, url(entry === "preview" ? PREVIEW_POINTER : INDEX_PATH), {
      cache: "no-cache",
    })

  const chooseEntry = async (
    bytes: Promise<Uint8Array>,
  ): Promise<{ chosen: JsonObject; source: "current" | "previous" }> => {
    if (entry === "preview") {
      const pointer = objectValue(parseJson(await bytes))
      return {
        chosen: {
          manifest_path: pointer["manifest_path"] ?? null,
          manifest_sha256: pointer["manifest_sha256"] ?? null,
        },
        source: "current",
      }
    }
    const index = objectValue(parseJson(await bytes))
    requireIndexFormat(index, "the version index")
    // no-cache still lets a lagging CDN edge answer; within a session the index never goes back,
    // and one revision always has the same content.
    const revision = integerValue(index["revision"])
    if (revision < lastRevision)
      throw new SnapshotError("blob-integrity", "version index revision went backwards")
    const signature = canonicalText(index)
    if (revision === lastRevision && lastIndex !== signature)
      throw new SnapshotError("blob-integrity", "version index changed without a new revision")
    const { current, previous } = indexEntries(index)
    lastRevision = revision
    lastIndex = signature
    for (const [source, candidate] of [
      ["current", current],
      ["previous", previous],
    ] as const) {
      if (candidate && isCompatible(candidate)) {
        return { chosen: candidate, source }
      }
    }
    throw new NoCompatibleVersion("no current or previous snapshot is compatible; update the site")
  }

  const matchEntry = (chosen: JsonObject, manifest: JsonObject): void => {
    if (entry === "index") checkEntry(chosen, manifest)
  }

  // A record holds what one full download verified, so it replaces the manifest, every startup
  // file and all of their checks; anything unreadable falls back to that download.
  const restore = async (
    manifestHash: string,
    selectedRegion = region,
  ): Promise<LoadedSnapshot | undefined> => {
    const bytes = await stored.read(`snapshot-${selectedRegion}`, manifestHash)
    if (!bytes) return undefined
    try {
      const record = objectValue(JSON.parse(new TextDecoder().decode(bytes)) as JsonValue)
      const manifest = objectValue(record["manifest"])
      return {
        dataVersion: stringValue(manifest["data_version"]),
        region: selectedRegion,
        manifestHash,
        manifest,
        // Rebuild the manifest file lookup without downloading it again.
        files: new Map(
          arrayValue(manifest["files"]).map((item) => {
            const file = objectValue(item)
            return [stringValue(file["key"]), file] as const
          }),
        ),
        config: objectValue(record["config"]),
        // Written from decoded fragments by `persist` below and digest-checked on read.
        bootstrap: arrayValue(record["bootstrap"]) as unknown as Fragment[],
      }
    } catch {
      await stored.remove(`snapshot-${selectedRegion}`, manifestHash)
      return undefined
    }
  }

  // Tens of megabytes of JSON would block input for a few hundred milliseconds in one task, so the
  // record is serialized in short slices once the page has rendered the new snapshot.
  const persist = async (snapshot: LoadedSnapshot): Promise<void> => {
    await new Promise((resolve) => {
      if (typeof requestIdleCallback === "function") requestIdleCallback(resolve, { timeout: 5000 })
      else setTimeout(resolve, 0)
    })
    const encoder = new TextEncoder()
    const { manifest, config, bootstrap } = snapshot
    const parts = [
      encoder.encode(
        `{"manifest":${JSON.stringify(manifest)},"config":${JSON.stringify(config)},"bootstrap":[`,
      ),
    ]
    let slice = performance.now()
    for (const [index, fragment] of bootstrap.entries()) {
      parts.push(encoder.encode(`${index ? "," : ""}${JSON.stringify(fragment)}`))
      if (performance.now() - slice > 8) {
        await new Promise((resolve) => setTimeout(resolve, 0))
        slice = performance.now()
      }
    }
    parts.push(encoder.encode("]}"))
    const payload = new Uint8Array(parts.reduce((size, part) => size + part.length, 0))
    let offset = 0
    for (const part of parts) {
      payload.set(part, offset)
      offset += part.length
    }
    await stored.write(`snapshot-${snapshot.region ?? region}`, snapshot.manifestHash, payload)
  }

  const download = async (
    previous: LoadedSnapshot | null,
    entryBytes?: Promise<Uint8Array>,
  ): Promise<{ readonly snapshot: LoadedSnapshot; readonly downloaded: boolean }> => {
    const selectedRegion = region
    const progress = (phase: LoadPhase, files?: { done: number; total: number }) => {
      set(
        previous
          ? {
              ...(status.state === "ready" ? status : {}),
              state: "ready",
              dataVersion: previous.dataVersion,
              updating: phase,
            }
          : { state: "loading", phase, ...files },
      )
    }
    progress("index")
    const { chosen, source } = await chooseEntry(entryBytes ?? requestEntry())
    // A check that finds the same manifest keeps the loaded snapshot and its caches as they are.
    if (
      previous &&
      previous.region === selectedRegion &&
      previous.manifestHash === stringValue(chosen["manifest_sha256"]) &&
      (previous.entrySource ?? "current") === source
    ) {
      matchEntry(chosen, previous.manifest)
      return { snapshot: previous, downloaded: false }
    }
    const cached = catalogs.get(`${stringValue(chosen["manifest_sha256"])}/${selectedRegion}`)
    const restored =
      cached ?? (await restore(stringValue(chosen["manifest_sha256"]), selectedRegion))
    if (restored) {
      matchEntry(chosen, restored.manifest)
      return {
        snapshot: { ...restored, entrySource: source },
        downloaded: cached !== undefined && unsaved.delete(cached),
      }
    }
    progress("manifest")
    const manifestBytes = await fetchVerified(
      stringValue(chosen["manifest_path"]),
      stringValue(chosen["manifest_sha256"]),
    )
    const decodedManifest = await decode({ kind: "manifest", bytes: manifestBytes })
    if (decodedManifest.kind !== "manifest") throw new SnapshotError("shape", "expected manifest")
    const { manifest, files } = decodedManifest
    matchEntry(chosen, manifest)
    const version = stringValue(manifest["format_version"])
    const configKey = stringValue(objectValue(manifest["config_ref"])["key"])
    const bootstrap: Fragment[] = []
    let config: JsonObject | null = null
    // Startup files are fetched together: one at a time cost a full round trip each, about a
    // minute behind a real CDN. Results are applied in manifest order.
    const startup = catalogKeys(files, selectedRegion).map((key) => {
      const file = files.get(key)
      if (!file) throw new SnapshotError("payload-set", "missing catalog file")
      return [key, file] as const
    })
    // A first download shows how many files have arrived, so a slow start reads as progress
    // rather than a stuck page. A background update keeps the old data and needs no count.
    let done = 0
    const total = startup.filter(([, file]) => file["role"] === "bootstrap").length
    progress("bootstrap", { done, total })
    // One failed file cancels the rest of the batch, so a retry is not queued behind it.
    const batch = new AbortController()
    const results = await Promise.all(
      startup.map(async ([key, file]) => {
        const data = await fetchVerified(
          stringValue(file["path"]),
          stringValue(file["sha256"]),
          integerValue(file["bytes"]),
          batch.signal,
        )
        const result = await decode({ kind: "file", file, bytes: data, version })
        if (file["role"] === "bootstrap") done += 1
        if (!previous && !batch.signal.aborted && file["role"] === "bootstrap")
          progress("bootstrap", { done, total })
        return { key, result }
      }),
    ).catch((error: unknown) => {
      batch.abort()
      throw error
    })
    for (const { key, result } of results) {
      if (key === configKey && result.kind === "config") config = result.value
      else if (result.kind === "fragments") bootstrap.push(...result.fragments)
      else throw new SnapshotError("shape", "unexpected startup payload")
    }
    if (!config) throw new SnapshotError("config-programs-count", "config file missing")
    if (previous?.manifestHash === stringValue(chosen["manifest_sha256"])) {
      const present = new Set(bootstrap.map((f) => f.identity))
      bootstrap.push(...previous.bootstrap.filter((f) => !present.has(f.identity)))
    }
    const snapshot: LoadedSnapshot = {
      region: selectedRegion,
      entrySource: source,
      dataVersion: stringValue(manifest["data_version"]),
      manifestHash: stringValue(chosen["manifest_sha256"]),
      manifest,
      files,
      config,
      bootstrap,
    }
    validateCatalog(bootstrap)
    catalogOf(snapshot)
    return { snapshot, downloaded: true }
  }

  const adopt = (candidate: LoadedSnapshot): void => {
    const previous = loaded
    if (previous?.manifestHash === candidate.manifestHash) {
      const present = new Set(candidate.bootstrap.map((fragment) => fragment.identity))
      candidate = {
        ...candidate,
        bootstrap: [
          ...candidate.bootstrap,
          ...previous.bootstrap.filter((fragment) => !present.has(fragment.identity)),
        ],
      }
    }
    catalogOf(candidate)
    for (const [key, cached] of catalogs)
      if (cached.manifestHash !== candidate.manifestHash) catalogs.delete(key)
    catalogs.set(`${candidate.manifestHash}/${candidate.region ?? region}`, candidate)
    metadata?.dispose()
    detailAbort.abort()
    decode.cancel()
    detailAbort = new AbortController()
    loaded = candidate
    detailBytes = 0
    metadata = new MetadataBytes(
      base,
      candidate.manifestHash,
      new Map(
        [...candidate.files].filter(
          ([, file]) =>
            file["role"] === "images" &&
            arrayValue(file["row_counts"]).some((c) => arrayValue(c)[0] === "printing_image"),
        ),
      ),
      fetcher,
      () => {
        for (const listener of listeners) listener()
      },
      options.cacheStorage ?? globalThis.caches,
      requests.background,
      previous?.manifestHash,
    )
    details = new Map()
    inflight = new Map()
  }

  const remember = async (snapshot: LoadedSnapshot, writeSnapshot: boolean): Promise<void> => {
    const selection = {
      manifestHash: snapshot.manifestHash,
      source: snapshot.entrySource ?? "current",
      revision: lastRevision,
      index: lastIndex,
    }
    if (writeSnapshot) await persist(snapshot)
    // A slower serialization must not replace the selection of a newer adopted snapshot.
    if (
      loaded?.manifestHash !== snapshot.manifestHash ||
      loaded.region !== snapshot.region ||
      selection.revision !== lastRevision ||
      selection.index !== lastIndex
    )
      return
    await stored.write(
      `adopted-${entry}-${snapshot.region ?? region}`,
      "active",
      new TextEncoder().encode(JSON.stringify(selection)),
    )
  }

  const restoreAdopted = async (): Promise<LoadedSnapshot | undefined> => {
    const selectedRegion = region
    const bytes = await stored.read(`adopted-${entry}-${selectedRegion}`, "active")
    if (!bytes) return undefined
    try {
      const selection = objectValue(parseJson(bytes))
      const source = stringValue(selection["source"])
      if (source !== "current" && source !== "previous") return undefined
      const revision = integerValue(selection["revision"])
      const signature = stringValue(selection["index"])
      const snapshot = await restore(stringValue(selection["manifestHash"]), selectedRegion)
      if (!snapshot) return undefined
      const restored: LoadedSnapshot = { ...snapshot, entrySource: source }
      catalogOf(restored)
      if (revision > lastRevision) {
        lastRevision = revision
        lastIndex = signature
      }
      return restored
    } catch {
      await stored.remove(`adopted-${entry}-${selectedRegion}`, "active")
      return undefined
    }
  }

  // A failed background check leaves the verified local snapshot usable with an explicit error.
  const run = async (entryBytes?: Promise<Uint8Array>): Promise<void> => {
    const previous = loaded
    try {
      const { snapshot: candidate, downloaded } = await download(previous, entryBytes)
      if (candidate.region !== region) {
        await run()
        return
      }
      if (candidate !== previous) adopt(candidate)
      set({
        state: "ready",
        dataVersion: candidate.dataVersion,
        ...(candidate.entrySource === "previous" ? { outdated: true } : {}),
      })
      // A restored record is already stored; serializing tens of megabytes again only costs time.
      void remember(loaded ?? candidate, downloaded).catch(() => undefined)
    } catch (error) {
      const failure = failureOf(error)
      if (previous)
        set({
          state: "ready",
          dataVersion: previous.dataVersion,
          ...(status.state === "ready" && status.unconfirmed ? { unconfirmed: true } : {}),
          outdated: true,
          updateError: failure,
        })
      else set({ state: "error", ...failure })
    } finally {
      pending = null
    }
  }

  const start = async (): Promise<void> => {
    // Leave an earlier error at once, as the plain download path does, so a new load never shows
    // the previous failure while the local record is still being read.
    if (status.state === "error") set({ state: "loading", phase: "index" })
    try {
      const entryBytes = requestEntry()
      // Network can fail before local storage finishes; run still classifies the original rejection.
      void entryBytes.catch(() => undefined)
      const cached = await restoreAdopted()
      if (cached && cached.region === region) {
        adopt(cached)
        set({
          state: "ready",
          dataVersion: cached.dataVersion,
          unconfirmed: true,
          ...(cached.entrySource === "previous" ? { outdated: true } : {}),
        })
      }
      // Reconnection during restore must replace a request that may already have failed offline.
      pending = run(reloadRequested ? undefined : entryBytes)
      if (!cached) await pending
    } catch (error) {
      metadata?.dispose()
      metadata = undefined
      loaded = null
      set({ state: "error", ...failureOf(error) })
    } finally {
      startup = null
      reloadRequested = false
    }
  }

  const load = (): Promise<void> => {
    if (status.state === "ready") return Promise.resolve()
    if (startup) return startup
    if (pending) return pending
    startup = start()
    return startup
  }

  const facesOf = (snapshot: LoadedSnapshot): Map<string, JsonObject> => {
    const faces = new Map<string, JsonObject>()
    for (const fragment of snapshot.bootstrap) {
      if (fragment.table === "face")
        for (const row of fragment.rows) faces.set(stringValue(row["id"]), row)
    }
    return faces
  }

  const loadFile = async (
    snapshot: LoadedSnapshot,
    key: string,
    backgroundSignal?: AbortSignal,
  ): Promise<readonly Fragment[]> => {
    const file = snapshot.files.get(key)
    if (!file) throw new SnapshotError("payload-set", `unknown file ${key}`)
    const signal = backgroundSignal
      ? AbortSignal.any([detailAbort.signal, backgroundSignal])
      : detailAbort.signal
    const active = () => !signal.aborted && loaded?.manifestHash === snapshot.manifestHash
    const role = file["role"]
    if (role !== "text" && role !== "images" && role !== "bootstrap")
      throw new SnapshotError("payload-set", `${key} is not a detail file`)
    const data =
      role === "images" &&
      metadata &&
      arrayValue(file["row_counts"]).some((c) => arrayValue(c)[0] === "printing_image")
        ? await metadata.read(key, backgroundSignal === undefined, backgroundSignal)
        : await fetchVerified(
            stringValue(file["path"]),
            stringValue(file["sha256"]),
            integerValue(file["bytes"]),
            signal,
            backgroundSignal ? "low" : "foreground",
          )
    if (!active()) throw new Error("snapshot replaced")
    const version = stringValue(snapshot.manifest["format_version"])
    const result = await decode({ kind: "file", file, bytes: data, version })
    if (result.kind !== "fragments") throw new SnapshotError("shape", "expected fragments")
    const decoded = result.fragments
    if (!active()) throw new Error("snapshot replaced")
    const faces = decoded.some((fragment) => fragment.value["base"] !== null)
      ? facesOf(snapshot)
      : new Map<string, JsonObject>()
    const bases: Fragment[] = [...snapshot.bootstrap, ...decoded]
    for (const fragment of decoded) {
      if (fragment.value["base"] === null) continue
      const ref = objectValue(objectValue(fragment.value["base"])["file"])
      const baseKey = stringValue(ref["key"])
      if (snapshot.files.get(baseKey)?.["sha256"] !== ref["sha256"])
        throw new SnapshotError("base-missing", "exact base hash")
      if (!bases.some((f) => f.file === baseKey))
        bases.push(...(await fragments(baseKey, backgroundSignal)))
    }
    for (const fragment of bases)
      if (fragment.table === "face")
        for (const row of fragment.rows) faces.set(stringValue(row["id"]), row)
    return decoded.map((fragment) => {
      if (fragment.value["base"] === null) return fragment
      const base = findBase(fragment, bases)
      return { ...fragment, rows: joinDetail(fragment, base, faces) }
    })
  }

  const fragments: SnapshotClient["fragments"] = (fileKey, backgroundSignal) => {
    if (backgroundSignal?.aborted) return Promise.reject(new Error("background read cancelled"))
    const snapshot = loaded
    if (!snapshot) return Promise.reject(new SnapshotError("payload-set", "snapshot not loaded"))
    const cached = details.get(fileKey)
    if (cached) {
      // Re-insert so the most recently used file is evicted last.
      details.delete(fileKey)
      details.set(fileKey, cached)
      return Promise.resolve(cached)
    }
    const running = backgroundSignal ? undefined : inflight.get(fileKey)
    if (running) return running
    const started = inflight
    const promise = loadFile(snapshot, fileKey, backgroundSignal)
      .catch(async (error: unknown) => {
        if (
          backgroundSignal === undefined &&
          loaded === snapshot &&
          error instanceof NetworkError &&
          (error.status === 404 || error.status === 410)
        )
          await reload()
        throw error
      })
      .then((result) => {
        // Ignore a result for a snapshot that was replaced while the request ran.
        if (started === inflight && !backgroundSignal?.aborted) {
          if (snapshot.files.get(fileKey)?.["role"] === "images") return result
          if (!details.has(fileKey))
            detailBytes += integerValue(snapshot.files.get(fileKey)?.["bytes"])
          details.set(fileKey, result)
          while (details.size > cacheSize || detailBytes > 12 * 1024 * 1024) {
            const oldest = details.keys().next().value
            if (oldest === undefined) break
            details.delete(oldest)
            detailBytes -= integerValue(snapshot.files.get(oldest)?.["bytes"])
          }
        }
        return result
      })
      .finally(() => {
        if (!backgroundSignal && started === inflight) inflight.delete(fileKey)
      })
    if (!backgroundSignal) inflight.set(fileKey, promise)
    return promise
  }

  let printingPending: { key: string; promise: Promise<void> } | undefined
  const prefetchPrintings = (): Promise<void> => {
    const snapshot = loaded
    if (!snapshot || snapshot.printingReady) return Promise.resolve()
    const selectedRegion = snapshot.region ?? options.region ?? "jp"
    const key = `${snapshot.manifestHash}/${selectedRegion}`
    if (printingPending?.key === key) return printingPending.promise
    const promise = Promise.all(
      [...snapshot.files]
        .filter(
          ([, file]) =>
            ["print", "product"].includes(stringValue(file["purpose"])) &&
            (file["region"] === null || file["region"] === selectedRegion),
        )
        .map(([key]) => fragments(key)),
    )
      .then((values) => {
        if (loaded !== snapshot) return
        const present = new Set(snapshot.bootstrap.map((fragment) => fragment.identity))
        const supplemented: LoadedSnapshot = {
          ...snapshot,
          printingReady: true,
          bootstrap: [
            ...snapshot.bootstrap,
            ...values.flat().filter((fragment) => !present.has(fragment.identity)),
          ],
        }
        catalogOf(supplemented)
        loaded = supplemented
        if (status.state === "ready") set({ ...status })
      })
      .finally(() => {
        if (printingPending?.promise === promise) printingPending = undefined
      })
    printingPending = { key, promise }
    return promise
  }

  const reload = (): Promise<void> => {
    if (pending) return pending
    if (startup) {
      reloadRequested = true
      return startup.then(() => pending ?? Promise.resolve())
    }
    pending = run()
    return pending
  }

  const catalog = async (
    selectedRegion: "jp" | "en",
    signal: AbortSignal,
  ): Promise<LoadedSnapshot> => {
    await (startup ?? pending)
    signal.throwIfAborted()
    const snapshot = loaded
    // Lists hold a compact Worker index, so comparison must not download the selected catalog
    // again merely to obtain its manifest on the main thread.
    if (!snapshot) {
      const comparison = createSnapshotClient(base, {
        ...options,
        region: selectedRegion,
        fetch: (url, init) =>
          requests.background(url, {
            ...init,
            signal: init?.signal ? AbortSignal.any([signal, init.signal]) : signal,
          }),
      })
      await comparison.load()
      signal.throwIfAborted()
      const candidate = comparison.snapshot()
      if (!candidate) throw new SnapshotError("payload-set", "comparison catalog failed")
      catalogs.set(`${candidate.manifestHash}/${selectedRegion}`, candidate)
      return candidate
    }
    const key = `${snapshot.manifestHash}/${selectedRegion}`
    const cached = catalogs.get(key)
    if (cached) return cached
    const present = new Set(snapshot.bootstrap.map((fragment) => fragment.file))
    const version = stringValue(snapshot.manifest["format_version"])
    const parts = await Promise.all(
      catalogKeys(snapshot.files, selectedRegion)
        .filter((key) => !present.has(key) && snapshot.files.get(key)?.["purpose"] !== "config")
        .map(async (key) => {
          signal.throwIfAborted()
          const file = snapshot.files.get(key)
          if (!file) throw new SnapshotError("payload-set", "missing catalog file")
          const bytes = await fetchVerified(
            stringValue(file["path"]),
            stringValue(file["sha256"]),
            integerValue(file["bytes"]),
            signal,
            "background",
          )
          signal.throwIfAborted()
          const result = await decode({ kind: "file", file, bytes, version })
          if (result.kind !== "fragments")
            throw new SnapshotError("shape", "expected catalog fragments")
          return result.fragments
        }),
    )
    signal.throwIfAborted()
    if (loaded?.manifestHash !== snapshot.manifestHash) throw new Error("snapshot replaced")
    const candidate = {
      ...snapshot,
      region: selectedRegion,
      printingReady: false,
      bootstrap: [...snapshot.bootstrap, ...parts.flat()],
    }
    validateCatalog(candidate.bootstrap)
    catalogOf(candidate)
    unsaved.add(candidate)
    catalogs.set(key, candidate)
    return candidate
  }

  return {
    base,
    status: () => status,
    subscribe: (listener) => {
      listeners.add(listener)
      return () => {
        listeners.delete(listener)
      }
    },
    load,
    retry: () => {
      if (status.state === "error") status = { state: "idle" }
      return load()
    },
    reload,
    setRegion: (next) => {
      if (next === region) return
      if (loaded)
        void (async () => {
          region = next
          await reload()
        })()
      else region = next
    },
    selectRegion: async (next) => {
      const changed = next !== region
      region = next
      if (changed) await reload()
      else await (pending ?? startup ?? load())
      // An earlier download may have captured the previous region before this selection arrived.
      if (loaded && region === next && loaded.region !== next) await reload()
    },
    snapshot: () => loaded,
    catalog,
    fragments,
    metadataStatus: () =>
      metadata?.status() ?? { state: "idle", done: 0, total: 0, persistent: false },
    prefetchPrintings,
    prefetchImages: () => metadata?.prefetch() ?? Promise.resolve(),
    cancelImagePrefetch: () => metadata?.cancel(),
  }
}
