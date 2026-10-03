import { fetchBytes, type Fetcher, NetworkError } from "./cdn"
import { createDecoder } from "./format-v1/decoder"
import { SnapshotError } from "./format-v1/errors"
import {
  arrayValue,
  canonicalText,
  integerValue,
  type JsonObject,
  objectValue,
  parseStrict,
  stringValue,
} from "./format-v1/json"
import { validateMedia } from "./format-v1/media"
import { validatePlacement } from "./format-v1/placement"
import { type Files, findBase, type Fragment, isCompatible, joinDetail } from "./format-v1/reader"
import { validate } from "./format-v1/schema"
import { validateDigitalLinks, validateImageRows } from "./format-v1/semantics"
import { transferDigest } from "./integrity"
import { MetadataBytes, type MetadataProgress } from "./metadata"
import { requestQueue } from "./request-queue"

type LoadPhase = "index" | "manifest" | "bootstrap"
type ErrorKind = "network" | "incompatible" | "corrupt"
interface LoadFailure {
  readonly kind: ErrorKind
  readonly detail: string
}
export type SnapshotStatus =
  | { readonly state: "idle" }
  | { readonly state: "loading"; readonly phase: LoadPhase }
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
    }
  | ({ readonly state: "error" } & LoadFailure)

export interface LoadedSnapshot {
  readonly dataVersion: string
  readonly entrySource?: "current" | "previous"
  readonly manifestHash: string
  readonly manifest: JsonObject
  readonly files: Files
  readonly config: JsonObject
  /** Every bootstrap fragment, decoded; detail files are joined onto these on demand. */
  readonly bootstrap: readonly Fragment[]
}

export interface SnapshotClientOptions {
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
  /** Starts loading unless already loading or ready; resolves when the status settles. */
  readonly load: () => Promise<void>
  readonly retry: () => Promise<void>
  /** Reads the version index again even when ready; the active snapshot only changes on success. */
  readonly reload: () => Promise<void>
  readonly snapshot: () => LoadedSnapshot | null
  /** A text, history or images file, decoded and joined onto its bootstrap base; cached LRU. */
  readonly fragments: (fileKey: string) => Promise<readonly Fragment[]>
  readonly metadataStatus: () => MetadataProgress
  readonly prefetchImages: () => Promise<void>
  readonly cancelImagePrefetch: () => void
}

const INDEX_PATH = "snapshots/versions/index.json"
const INDEX_FORMAT = 2
// Preview roots have no permanent index (format §4.2); until #4 Q12 settles, dev reads this pointer.
const PREVIEW_POINTER = "snapshots/preview/current.json"

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

function validateIndex2(index: JsonObject): void {
  validate("Index", index, [], "2.0.0")
  const entries = [
    objectValue(index["current"]),
    ...(index["previous"] === null ? [] : [objectValue(index["previous"])]),
  ]
  for (const entry of entries) {
    const capabilities = arrayValue(entry["required_capabilities"]).map((value) =>
      stringValue(value),
    )
    if (
      new Set(capabilities).size !== capabilities.length ||
      canonicalText([...capabilities].sort()) !== canonicalText(capabilities)
    )
      throw new SnapshotError("schema", "Index capabilities must be sorted and unique")
    const hash = stringValue(entry["manifest_sha256"]).slice(7)
    if (entry["manifest_path"] !== `snapshots/manifests/${hash}.json`)
      throw new SnapshotError("blob-integrity", "Index manifest path differs from hash")
  }
}

function validateMediaFile(fragments: readonly Fragment[], snapshot: LoadedSnapshot): void {
  const media = fragments.filter((fragment) => fragment.table === "printing_image")
  if (media.length === 0) return
  const required = new Set([stringValue(objectValue(snapshot.manifest["config_ref"])["key"])])
  for (const fragment of media) {
    for (const row of fragment.rows) {
      for (const [table, field] of [
        ["printing", "printing_id"],
        ["face", "face_id"],
      ]) {
        const base = snapshot.bootstrap.find(
          (f) => f.table === table && f.rows.some((r) => r["id"] === row[field ?? ""]),
        )
        if (!base) throw new SnapshotError("dangling-reference", "media bootstrap target missing")
        if (
          table === "printing" &&
          canonicalText(base.value["owner"] ?? null) !==
            canonicalText(fragment.value["owner"] ?? null)
        )
          throw new SnapshotError("printing-owner-conflict", "media differs from printing home_set")
        required.add(base.file)
      }
    }
  }
  // Dependencies belong to the physical file, which can pack many logical media fragments.
  const deps = arrayValue(snapshot.files.get(media[0]?.file ?? "")?.["dependencies"]).map((d) =>
    stringValue(objectValue(d)["key"]),
  )
  if (canonicalText(deps) !== canonicalText([...required].sort()))
    throw new SnapshotError(
      "dependency-closure",
      "media requires exact config/printing/face dependencies",
    )
}

export function createSnapshotClient(
  base: string,
  options: SnapshotClientOptions = {},
): SnapshotClient {
  const requests = requestQueue(options.fetch ?? ((url, init) => fetch(url, init)))
  const fetcher = requests.foreground
  const entry = options.entry ?? "index"
  const decode = createDecoder()
  const cacheSize = options.detailCacheSize ?? 64
  let metadata: MetadataBytes | undefined
  let detailBytes = 0
  const listeners = new Set<() => void>()
  let status: SnapshotStatus = { state: "idle" }
  let loaded: LoadedSnapshot | null = null
  let pending: Promise<void> | null = null
  let lastRevision = -1
  let lastIndex = ""
  let detailAbort = new AbortController()
  // Settled detail files in LRU order; requests still in flight live apart so eviction never
  // drops a promise other callers are waiting on.
  let details = new Map<string, readonly Fragment[]>()
  let inflight = new Map<string, Promise<readonly Fragment[]>>()

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
  ): Promise<Uint8Array> => {
    for (let attempt = 0; ; attempt += 1) {
      const data = await fetchBytes(fetcher, url(path), signal ? { signal } : undefined)
      if ((await transferDigest(data)) === sha256 && (bytes === undefined || data.length === bytes))
        return data
      if (attempt === 1)
        throw new SnapshotError("blob-integrity", `hash mismatch for ${path} after retry`)
    }
  }

  // Only a newer integer format is "update the site"; anything else is a broken index (schema).
  const requireIndexFormat = (value: JsonObject, what: string): void => {
    const format = value["index_format"]
    if (typeof format === "number" && Number.isInteger(format) && format > INDEX_FORMAT) {
      throw new NoCompatibleVersion(
        `${what} uses an index format this version of the site cannot read`,
      )
    }
  }

  const chooseEntry = async (): Promise<{ chosen: JsonObject; source: "current" | "previous" }> => {
    if (entry === "preview") {
      const pointer = objectValue(
        parseStrict(await fetchBytes(fetcher, url(PREVIEW_POINTER), { cache: "no-cache" })),
      )
      return {
        chosen: {
          manifest_path: pointer["manifest_path"] ?? null,
          manifest_sha256: pointer["manifest_sha256"] ?? null,
        },
        source: "current",
      }
    }
    const index = objectValue(
      parseStrict(await fetchBytes(fetcher, url(INDEX_PATH), { cache: "no-cache" })),
    )
    requireIndexFormat(index, "the version index")
    const modern = index["index_format"] === 2
    if (modern) validateIndex2(index)
    else validate("Index", index)
    const revision = integerValue(index["revision"])
    if (revision < lastRevision)
      throw new SnapshotError("blob-integrity", "version index revision went backwards")
    const signature = canonicalText(index)
    if (revision === lastRevision && lastIndex !== signature)
      throw new SnapshotError("blob-integrity", "version index changed without a new revision")
    if (modern) {
      const current = objectValue(index["current"])
      const previous = index["previous"] === null ? null : objectValue(index["previous"])
      if (
        previous &&
        (current["data_version"] === previous["data_version"] ||
          stringValue(current["published_at"]) < stringValue(previous["published_at"]))
      )
        throw new SnapshotError("blob-integrity", "invalid current/previous ordering")
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
      throw new NoCompatibleVersion(
        "no current or previous snapshot is compatible; update the site",
      )
    }
    const pages = arrayValue(index["pages"]).map((page) => objectValue(page))
    for (const page of pages.reverse()) {
      const data = await fetchVerified(stringValue(page["path"]), stringValue(page["sha256"]))
      const value = objectValue(parseStrict(data))
      requireIndexFormat(value, "a version index page")
      validate("IndexPage", value)
      const entries = arrayValue(value["entries"]).map((item) => objectValue(item))
      for (const candidate of entries.reverse()) {
        if (isCompatible(candidate)) {
          lastRevision = revision
          lastIndex = signature
          return { chosen: candidate, source: "current" }
        }
      }
    }
    throw new NoCompatibleVersion("no published snapshot is readable by this version of the site")
  }

  const download = async (previous: LoadedSnapshot | null): Promise<LoadedSnapshot> => {
    const progress = (phase: LoadPhase) => {
      set(
        previous
          ? {
              ...(status.state === "ready" ? status : {}),
              state: "ready",
              dataVersion: previous.dataVersion,
              updating: phase,
            }
          : { state: "loading", phase },
      )
    }
    progress("index")
    const { chosen, source } = await chooseEntry()
    progress("manifest")
    const manifestBytes = await fetchVerified(
      stringValue(chosen["manifest_path"]),
      stringValue(chosen["manifest_sha256"]),
    )
    const decodedManifest = await decode({ kind: "manifest", bytes: manifestBytes })
    if (decodedManifest.kind !== "manifest") throw new SnapshotError("shape", "expected manifest")
    const { manifest, files } = decodedManifest
    if (entry !== "preview") {
      for (const field of [
        "data_version",
        "published_at",
        "format_version",
        "min_reader_version",
        "required_capabilities",
        "engine_support_target",
      ])
        if (canonicalText(chosen[field] ?? null) !== canonicalText(manifest[field] ?? null))
          throw new SnapshotError("blob-integrity", "index entry differs from verified manifest")
    }
    const version = stringValue(manifest["format_version"])
    progress("bootstrap")
    const configKey = stringValue(objectValue(manifest["config_ref"])["key"])
    const bootstrap: Fragment[] = []
    let config: JsonObject | null = null
    for (const [key, file] of files) {
      const role = file["role"]
      if (role !== "config" && role !== "bootstrap") continue
      const data = await fetchVerified(
        stringValue(file["path"]),
        stringValue(file["sha256"]),
        integerValue(file["bytes"]),
      )
      const result = await decode({ kind: "file", file, bytes: data, version })
      if (key === configKey && result.kind === "config") config = result.value
      else if (result.kind === "fragments") bootstrap.push(...result.fragments)
      else throw new SnapshotError("shape", "unexpected startup payload")
    }
    if (!config) throw new SnapshotError("config-programs-count", "config file missing")
    const faceCards = new Map(
      bootstrap
        .filter((fragment) => fragment.table === "face")
        .flatMap((fragment) =>
          fragment.rows.map((row) => [stringValue(row["id"]), row["card_id"] ?? null] as const),
        ),
    )
    let segment = performance.now()
    for (const fragment of bootstrap) {
      validatePlacement([fragment], version, bootstrap, faceCards)
      // Leave time for input and rendering between bounded validation segments.
      if (typeof Worker !== "undefined" && performance.now() - segment > 8) {
        await new Promise<void>((resolve) => setTimeout(resolve, 0))
        segment = performance.now()
      }
    }
    return {
      entrySource: source,
      dataVersion: stringValue(manifest["data_version"]),
      manifestHash: stringValue(chosen["manifest_sha256"]),
      manifest,
      files,
      config,
      bootstrap,
    }
  }

  // The active snapshot only changes once a candidate is fully downloaded and verified; a failed
  // reload keeps the previous one usable and reports the failure on the ready status.
  const run = async (): Promise<void> => {
    const previous = loaded
    try {
      const candidate = await download(previous)
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
              (candidate.manifest["format_version"] !== "2.0.0" ||
                arrayValue(file["row_counts"]).some(
                  (c) => objectValue(c)["table"] === "printing_image",
                )),
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
      set({
        state: "ready",
        dataVersion: candidate.dataVersion,
        ...(candidate.entrySource === "previous" ? { outdated: true } : {}),
      })
    } catch (error) {
      const failure = failureOf(error)
      if (previous)
        set({
          state: "ready",
          dataVersion: previous.dataVersion,
          outdated: true,
          updateError: failure,
        })
      else set({ state: "error", ...failure })
    } finally {
      pending = null
    }
  }

  const load = (): Promise<void> => {
    if (pending) return pending
    if (status.state === "ready") return Promise.resolve()
    pending = run()
    return pending
  }

  const facesOf = (snapshot: LoadedSnapshot): Map<string, JsonObject> => {
    const faces = new Map<string, JsonObject>()
    for (const fragment of snapshot.bootstrap) {
      if (fragment.table === "face")
        for (const row of fragment.rows) faces.set(stringValue(row["id"]), row)
    }
    return faces
  }

  const loadFile = async (snapshot: LoadedSnapshot, key: string): Promise<readonly Fragment[]> => {
    const file = snapshot.files.get(key)
    if (!file) throw new SnapshotError("payload-set", `unknown file ${key}`)
    const signal = detailAbort.signal
    const active = () => !signal.aborted && loaded === snapshot
    const role = file["role"]
    if (role !== "text" && role !== "images")
      throw new SnapshotError("payload-set", `${key} is not a detail file`)
    const data =
      role === "images" &&
      metadata &&
      (snapshot.manifest["format_version"] !== "2.0.0" ||
        arrayValue(file["row_counts"]).some((c) => objectValue(c)["table"] === "printing_image"))
        ? await metadata.read(key)
        : await fetchVerified(
            stringValue(file["path"]),
            stringValue(file["sha256"]),
            integerValue(file["bytes"]),
            signal,
          )
    if (!active()) throw new Error("snapshot replaced")
    const version = stringValue(snapshot.manifest["format_version"])
    const result = await decode({ kind: "file", file, bytes: data, version })
    if (result.kind !== "fragments") throw new SnapshotError("shape", "expected fragments")
    const fragments = result.fragments
    validatePlacement(fragments, version, snapshot.bootstrap)
    if (role === "images") {
      const view: Record<string, JsonObject[]> = {}
      for (const fragment of [
        ...(fragments.some((fragment) => fragment.table === "printing_image")
          ? snapshot.bootstrap.filter((fragment) => fragment.table === "printing")
          : []),
        ...fragments,
      ])
        (view[fragment.table] ??= []).push(...fragment.rows)
      validateImageRows(view)
      if (version === "2.0.0") {
        for (const row of view["printing_image"] ?? []) validateMedia(row)
        validateMediaFile(fragments, snapshot)
      }
    }
    const digitalView: Record<string, JsonObject[]> = {}
    for (const fragment of fragments) (digitalView[fragment.table] ??= []).push(...fragment.rows)
    validateDigitalLinks(digitalView)
    if (!active()) throw new Error("snapshot replaced")
    const faces = facesOf(snapshot)
    return fragments.map((fragment) => {
      if (fragment.value["base"] === null) return fragment
      const base = findBase(fragment, [...snapshot.bootstrap, ...fragments], snapshot.files)
      return { ...fragment, rows: joinDetail(fragment, base, faces) }
    })
  }

  const fragments = (fileKey: string): Promise<readonly Fragment[]> => {
    const snapshot = loaded
    if (!snapshot) return Promise.reject(new SnapshotError("payload-set", "snapshot not loaded"))
    const cached = details.get(fileKey)
    if (cached) {
      // Re-insert so the most recently used file is evicted last.
      details.delete(fileKey)
      details.set(fileKey, cached)
      return Promise.resolve(cached)
    }
    const running = inflight.get(fileKey)
    if (running) return running
    const started = inflight
    const promise = loadFile(snapshot, fileKey)
      .catch(async (error: unknown) => {
        if (
          loaded === snapshot &&
          error instanceof NetworkError &&
          (error.status === 404 || error.status === 410)
        )
          await reload()
        throw error
      })
      .then((result) => {
        // Ignore a result for a snapshot that was replaced while the request ran.
        if (started === inflight) {
          if (snapshot.files.get(fileKey)?.["role"] === "images") return result
          details.set(fileKey, result)
          detailBytes += integerValue(snapshot.files.get(fileKey)?.["bytes"])
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
        if (started === inflight) inflight.delete(fileKey)
      })
    inflight.set(fileKey, promise)
    return promise
  }

  const reload = (): Promise<void> => {
    if (pending) return pending
    pending = run()
    return pending
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
    snapshot: () => loaded,
    fragments,
    metadataStatus: () =>
      metadata?.status() ?? { state: "idle", done: 0, total: 0, persistent: false },
    prefetchImages: () => metadata?.prefetch() ?? Promise.resolve(),
    cancelImagePrefetch: () => metadata?.cancel(),
  }
}
