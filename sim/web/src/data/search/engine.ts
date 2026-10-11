import type { Region } from "../../domain/search"
import { fetchBytes, type Fetcher } from "../cdn"
import { decodeMessage } from "../format-v3/decode-message"
import { SnapshotError } from "../format-v3/errors"
import {
  arrayValue,
  integerValue,
  type JsonObject,
  objectValue,
  parseJson,
  stringValue,
} from "../format-v3/json"
import { isCompatible } from "../format-v3/reader"
import { transferDigest } from "../integrity"
import { VerifiedCache } from "../verified-cache"
import { SearchBlobs } from "./blobs"
import { BootstrapColumns } from "./bootstrap"
import { pack, unpack } from "./image"
import { INDEX_IMAGE_FORMAT, SearchIndex, SearchQueryError } from "./index"
import type { BootstrapPlan, LoadRequest, SearchEvent, SearchRequest } from "./messages"

const defaultPlan: BootstrapPlan = (_manifest, files) =>
  [...files].filter(([, file]) => file["role"] === "bootstrap").map(([key]) => key)
/** A stored index with the metadata needed by the ready event. */
function readRecord(bytes: Uint8Array, edition: Region) {
  const { header, arrays } = unpack(bytes)
  const record = objectValue(header)
  const [image] = arrays
  if (arrays.length !== 1 || !(image instanceof Uint8Array))
    throw new Error("stored search record has the wrong layout")
  return {
    index: new SearchIndex(image, edition),
    dataVersion: stringValue(record["dataVersion"]),
    files: integerValue(record["files"]),
  }
}

/** Fetch and parse are sequential; a replacement load aborts and drains the previous staging job. */
export class SearchEngine {
  private readonly emit: (event: SearchEvent) => void
  private readonly fetcher: Fetcher
  private readonly storage: CacheStorage | undefined
  private readonly plan: BootstrapPlan
  private readonly buildId: string | undefined
  private staging: { generation: number; abort: AbortController } | undefined
  private active: { generation: number; index: SearchIndex } | undefined
  private readonly queries = new Map<
    number,
    { request: Extract<SearchRequest, { kind: "search" }>; settled: () => void }
  >()
  private queryTimer: ReturnType<typeof setTimeout> | undefined
  private pending: Promise<void> = Promise.resolve()

  constructor(
    emit: (event: SearchEvent) => void,
    options: {
      fetch?: Fetcher
      cacheStorage?: CacheStorage
      plan?: BootstrapPlan
      buildId?: string
    } = {},
  ) {
    this.emit = emit
    this.fetcher = options.fetch ?? ((url, init) => fetch(url, init))
    this.storage = options.cacheStorage ?? globalThis.caches
    this.plan = options.plan ?? defaultPlan
    this.buildId = options.buildId
  }

  handle(request: SearchRequest): Promise<void> {
    if (request.kind === "cancel") {
      if (this.staging?.generation === request.generation) this.staging.abort.abort()
      return Promise.resolve()
    }
    if (request.kind === "close-channel") {
      this.queries.get(request.channelId)?.settled()
      this.queries.delete(request.channelId)
      return Promise.resolve()
    }
    if (request.kind === "search") {
      if (this.active?.generation !== request.generation) return Promise.resolve()
      const previous = this.queries.get(request.channelId)
      if (previous && previous.request.queryId >= request.queryId) return Promise.resolve()
      previous?.settled()
      const result = new Promise<void>((settled) =>
        this.queries.set(request.channelId, { request, settled }),
      )
      // A task boundary admits queued messages before scanning; each channel keeps its latest input.
      this.queryTimer ??= setTimeout(() => {
        this.flushQueries()
      }, 0)
      return result
    }
    this.staging?.abort.abort()
    const staging = { generation: request.generation, abort: new AbortController() }
    this.staging = staging
    this.pending = this.pending.then(() => this.load(request, staging.abort.signal))
    return this.pending
  }

  private flushQueries(): void {
    this.queryTimer = undefined
    const queries = [...this.queries.values()]
    this.queries.clear()
    for (const { request, settled } of queries) {
      try {
        if (this.active?.generation !== request.generation) continue
        this.emit({
          kind: "result",
          generation: request.generation,
          channelId: request.channelId,
          queryId: request.queryId,
          page: this.active.index.search(request.query),
        })
      } catch (error) {
        this.emit({
          kind: "query-error",
          generation: request.generation,
          channelId: request.channelId,
          queryId: request.queryId,
          error:
            error instanceof SearchQueryError
              ? error.failure
              : {
                  kind: "query-failed",
                  message: error instanceof Error ? error.message : "search failed",
                },
        })
      } finally {
        settled()
      }
    }
  }

  private async resolve(request: LoadRequest, signal: AbortSignal): Promise<JsonObject> {
    const pointer = objectValue(
      parseJson(
        await fetchBytes(
          this.fetcher,
          `${request.base}/snapshots/${request.entry === "preview" ? "preview/current.json" : "versions/index.json"}`,
          { signal, cache: "no-cache" },
        ),
      ),
    )
    if (request.entry === "preview") return pointer
    if (integerValue(pointer["index_format"]) !== 2)
      throw new SnapshotError("unsupported-version", "unsupported index format")
    const current = objectValue(pointer["current"])
    const previous = pointer["previous"] === null ? null : objectValue(pointer["previous"])
    const compatible = [current, previous].find((candidate) => candidate && isCompatible(candidate))
    if (!compatible) throw new SnapshotError("unsupported-version", "no compatible snapshot")
    return compatible
  }

  private async prepare(request: LoadRequest, entry: JsonObject, signal: AbortSignal) {
    const manifestHash = stringValue(entry["manifest_sha256"])
    const manifestBytes = await fetchBytes(
      this.fetcher,
      `${request.base}/${stringValue(entry["manifest_path"])}`,
      { signal },
    )
    signal.throwIfAborted()
    if ((await transferDigest(manifestBytes)) !== manifestHash)
      throw new SnapshotError("blob-integrity", "manifest hash mismatch")
    const mark = performance.now()
    const decoded = decodeMessage({ kind: "manifest", bytes: manifestBytes })
    const parseMs = performance.now() - mark
    if (decoded.kind !== "manifest") throw new SnapshotError("shape", "expected manifest")
    const { manifest, files } = decoded
    const version = stringValue(manifest["format_version"])
    const configKey = stringValue(objectValue(manifest["config_ref"])["key"])
    const keys = [
      configKey,
      ...new Set(this.plan(manifest, files, request.edition).filter((key) => key !== configKey)),
    ]
    return {
      manifestHash,
      version,
      configKey,
      keys,
      files: new Map(keys.map((key) => [key, files.get(key)] as const)),
      dataVersion: stringValue(manifest["data_version"]),
      parseMs,
      manifestRawBytes: manifestBytes.length,
    }
  }

  // Reading the stored image replaces the manifest, every blob and the index build.
  private async restore(
    request: LoadRequest,
    entry: JsonObject,
    stored: VerifiedCache,
    start: number,
    signal: AbortSignal,
  ): Promise<boolean> {
    const bytes = await stored.read(request.edition, stringValue(entry["manifest_sha256"]))
    if (!bytes) return false
    const mark = performance.now()
    let record: ReturnType<typeof readRecord>
    try {
      record = readRecord(bytes, request.edition)
    } catch {
      await stored.remove(request.edition, stringValue(entry["manifest_sha256"]))
      return false
    }
    const parseMs = performance.now() - mark
    signal.throwIfAborted()
    this.active = { generation: request.generation, index: record.index }
    this.emit({
      kind: "ready",
      generation: request.generation,
      edition: request.edition,
      root: request.base,
      dataVersion: record.dataVersion,
      manifestHash: stringValue(entry["manifest_sha256"]),
      metrics: {
        restored: true,
        files: record.files,
        maxRawBytes: bytes.length,
        parseMs,
        buildMs: 0,
        wallMs: performance.now() - start,
        firstFileMs: 0,
        firstSetMs: 0,
        stagingArrayBuffers: 0,
        stagingStringBytes: 0,
        ...record.index.allocation(),
      },
    })
    return true
  }

  private async load(request: LoadRequest, signal: AbortSignal): Promise<void> {
    const start = performance.now()
    try {
      signal.throwIfAborted()
      const entry = await this.resolve(request, signal)
      // A custom plan picks other files from the same manifest, so only the default plan, which the
      // build identity already covers, may share stored images.
      const stored = new VerifiedCache(
        request.base,
        "search-index",
        INDEX_IMAGE_FORMAT,
        this.plan === defaultPlan ? this.storage : undefined,
        this.buildId,
      )
      if (await this.restore(request, entry, stored, start, signal)) return
      const {
        manifestHash,
        version,
        configKey,
        keys,
        files,
        dataVersion,
        manifestRawBytes,
        parseMs: manifestParseMs,
      } = await this.prepare(request, entry, signal)
      let parseMs = manifestParseMs
      let mark = performance.now()
      const blobs = new SearchBlobs(request.base, this.fetcher, this.storage)
      let bootstrap: BootstrapColumns | undefined = new BootstrapColumns()
      let config: JsonObject | undefined
      let maxRawBytes = manifestRawBytes
      let firstFileMs = 0
      let firstSetMs = 0
      let buildMs = 0
      const remainingSets = new Map<string, number>()
      const ownersOf = (file: JsonObject) =>
        new Set(
          arrayValue(file["row_counts"])
            .map((count) => objectValue(objectValue(count)["owner"]))
            .filter((owner) => owner["kind"] === "home_set")
            .map((owner) => stringValue(owner["id"])),
        )
      for (const key of keys) {
        const file = files.get(key)
        if (file)
          for (const owner of ownersOf(file))
            remainingSets.set(owner, (remainingSets.get(owner) ?? 0) + 1)
      }
      this.emit({
        kind: "progress",
        generation: request.generation,
        done: 0,
        total: keys.length,
        persistent: blobs.persistent,
      })
      for (const [ordinal, key] of keys.entries()) {
        signal.throwIfAborted()
        const file = files.get(key)
        if (!file) throw new SnapshotError("payload-set", "load plan references unknown file")
        const bytes = await blobs.read(file, signal)
        maxRawBytes = Math.max(maxRawBytes, bytes.length)
        mark = performance.now()
        const result = decodeMessage({ kind: "file", bytes, file, version })
        if (key === configKey && result.kind === "config") config = result.value
        else if (result.kind !== "fragments")
          throw new SnapshotError("shape", "unexpected basic catalog payload")
        parseMs += performance.now() - mark
        mark = performance.now()
        if (result.kind === "fragments") bootstrap.ingest(result.fragments)
        buildMs += performance.now() - mark
        if (ordinal === 0) firstFileMs = performance.now() - start
        for (const owner of ownersOf(file)) {
          const remaining = (remainingSets.get(owner) ?? 1) - 1
          remainingSets.set(owner, remaining)
          if (!remaining && !firstSetMs) firstSetMs = performance.now() - start
        }
        this.emit({
          kind: "progress",
          generation: request.generation,
          done: ordinal + 1,
          total: keys.length,
          persistent: blobs.persistent,
        })
      }
      signal.throwIfAborted()
      if (!config) throw new SnapshotError("config-programs-count", "config missing")
      mark = performance.now()
      bootstrap.seal()
      const stagingAllocation = bootstrap.allocation()
      const index = new SearchIndex(bootstrap, request.edition)
      buildMs += performance.now() - mark
      signal.throwIfAborted()
      bootstrap = undefined
      this.active = { generation: request.generation, index }
      this.emit({
        kind: "ready",
        generation: request.generation,
        edition: request.edition,
        root: request.base,
        dataVersion,
        manifestHash,
        metrics: {
          restored: false,
          files: keys.length,
          maxRawBytes,
          parseMs,
          buildMs,
          wallMs: performance.now() - start,
          firstFileMs,
          firstSetMs,
          stagingArrayBuffers: stagingAllocation.arrayBuffers,
          stagingStringBytes: stagingAllocation.stringBytes,
          ...index.allocation(),
        },
      })
      await blobs.retainPlan(
        request.edition,
        [...files.values()].filter((file): file is JsonObject => file !== undefined),
      )
      // Written only now: every file passed its checks and this generation is already published.
      await stored.write(
        request.edition,
        manifestHash,
        pack({ dataVersion, files: keys.length }, [index.encode()]),
      )
    } catch (error) {
      if (!signal.aborted)
        this.emit({
          kind: "error",
          generation: request.generation,
          message: error instanceof Error ? error.message : "catalog load failed",
        })
    } finally {
      if (this.staging?.generation === request.generation) this.staging = undefined
    }
  }
}
