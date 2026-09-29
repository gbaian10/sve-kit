import { fetchBytes, type Fetcher, NetworkError } from "./cdn"
import { SnapshotError } from "./format-v1/errors"
import {
  arrayValue,
  integerValue,
  type JsonObject,
  objectValue,
  parseStrict,
  stringValue,
} from "./format-v1/json"
import {
  type Files,
  findBase,
  type Fragment,
  isCompatible,
  joinDetail,
  readContainer,
  readPayload,
  verifyManifest,
} from "./format-v1/reader"
import { validate } from "./format-v1/schema"
import { validateConfig, validateFragments } from "./format-v1/semantics"
import { digest } from "./format-v1/sha256"

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
    }
  | ({ readonly state: "error" } & LoadFailure)

export interface LoadedSnapshot {
  readonly dataVersion: string
  readonly manifest: JsonObject
  readonly files: Files
  readonly config: JsonObject
  /** Every bootstrap fragment, decoded; detail files are joined onto these on demand. */
  readonly bootstrap: readonly Fragment[]
}

export interface SnapshotClientOptions {
  readonly fetch?: Fetcher
  /** `index`: the permanent version index (format §4.1); `preview`: a preview root's pointer file. */
  readonly entry?: "index" | "preview"
  readonly detailCacheSize?: number
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
}

const INDEX_PATH = "snapshots/versions/index.json"
const INDEX_FORMAT = 1
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

export function createSnapshotClient(
  base: string,
  options: SnapshotClientOptions = {},
): SnapshotClient {
  const fetcher: Fetcher = options.fetch ?? ((url, init) => fetch(url, init))
  const entry = options.entry ?? "index"
  const cacheSize = options.detailCacheSize ?? 8
  const listeners = new Set<() => void>()
  let status: SnapshotStatus = { state: "idle" }
  let loaded: LoadedSnapshot | null = null
  let pending: Promise<void> | null = null
  let lastRevision = -1
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
  ): Promise<Uint8Array> => {
    for (let attempt = 0; ; attempt += 1) {
      const data = await fetchBytes(fetcher, url(path))
      if (digest(data) === sha256 && (bytes === undefined || data.length === bytes)) return data
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

  const chooseEntry = async (): Promise<JsonObject> => {
    if (entry === "preview") {
      const pointer = objectValue(
        parseStrict(await fetchBytes(fetcher, url(PREVIEW_POINTER), { cache: "no-cache" })),
      )
      return {
        manifest_path: pointer["manifest_path"] ?? null,
        manifest_sha256: pointer["manifest_sha256"] ?? null,
      }
    }
    const index = objectValue(
      parseStrict(await fetchBytes(fetcher, url(INDEX_PATH), { cache: "no-cache" })),
    )
    requireIndexFormat(index, "the version index")
    validate("Index", index)
    const revision = integerValue(index["revision"])
    if (revision < lastRevision)
      throw new SnapshotError("blob-integrity", "version index revision went backwards")
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
          return candidate
        }
      }
    }
    throw new NoCompatibleVersion("no published snapshot is readable by this version of the site")
  }

  const download = async (previous: LoadedSnapshot | null): Promise<LoadedSnapshot> => {
    const progress = (phase: LoadPhase) => {
      set(
        previous
          ? { state: "ready", dataVersion: previous.dataVersion, updating: phase }
          : { state: "loading", phase },
      )
    }
    progress("index")
    const chosen = await chooseEntry()
    progress("manifest")
    const manifestBytes = await fetchVerified(
      stringValue(chosen["manifest_path"]),
      stringValue(chosen["manifest_sha256"]),
    )
    const { manifest, files } = verifyManifest(parseStrict(manifestBytes))
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
      const value = objectValue(readPayload(file, data))
      if (key === configKey) {
        validate("Config", value, [key])
        validateConfig(value)
        config = value
      } else {
        bootstrap.push(...readContainer(file, value))
      }
    }
    if (!config) throw new SnapshotError("config-programs-count", "config file missing")
    validateFragments(bootstrap)
    return {
      dataVersion: stringValue(manifest["data_version"]),
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
      loaded = candidate
      details = new Map()
      inflight = new Map()
      set({ state: "ready", dataVersion: candidate.dataVersion })
    } catch (error) {
      const failure = failureOf(error)
      if (previous) set({ state: "ready", dataVersion: previous.dataVersion, updateError: failure })
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
    const role = file["role"]
    if (role !== "text" && role !== "images")
      throw new SnapshotError("payload-set", `${key} is not a detail file`)
    const data = await fetchVerified(
      stringValue(file["path"]),
      stringValue(file["sha256"]),
      integerValue(file["bytes"]),
    )
    const fragments = readContainer(file, objectValue(readPayload(file, data)))
    validateFragments(fragments)
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
      .then((result) => {
        // Ignore a result for a snapshot that was replaced while the request ran.
        if (started === inflight) {
          details.set(fileKey, result)
          while (details.size > cacheSize) {
            const oldest = details.keys().next().value
            if (oldest === undefined) break
            details.delete(oldest)
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
    reload: () => {
      if (pending) return pending
      pending = run()
      return pending
    },
    snapshot: () => loaded,
    fragments,
  }
}
