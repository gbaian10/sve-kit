import { catalogFromIndex } from "../catalog"
import { fetchBytes, type Fetcher } from "../cdn"
import { decodeMessage } from "../format-v3/decode-message"
import { SnapshotError } from "../format-v3/errors"
import {
  arrayValue,
  canonicalText,
  type JsonObject,
  objectValue,
  parseStrict,
  stringValue,
} from "../format-v3/json"
import { validateMediaIdentities } from "../format-v3/media"
import { validatePlacement } from "../format-v3/placement"
import { isCompatible } from "../format-v3/reader"
import { validateIndex2 } from "../index-entry"
import { transferDigest } from "../integrity"
import { SearchBlobs } from "./blobs"
import { BootstrapColumns } from "./bootstrap"
import { SearchIndex } from "./index"
import type { BootstrapPlan, LoadRequest, SearchEvent, SearchRequest } from "./messages"

const defaultPlan: BootstrapPlan = (_manifest, files) =>
  [...files].filter(([, file]) => file["role"] === "bootstrap").map(([key]) => key)

/** Fetch and parse are sequential; a replacement load aborts and drains the previous staging job. */
export class SearchEngine {
  private readonly emit: (event: SearchEvent) => void
  private readonly fetcher: Fetcher
  private readonly storage: CacheStorage | undefined
  private readonly plan: BootstrapPlan
  private staging: { generation: number; abort: AbortController } | undefined
  private active: { generation: number; index: SearchIndex } | undefined
  private pending: Promise<void> = Promise.resolve()

  constructor(
    emit: (event: SearchEvent) => void,
    options: { fetch?: Fetcher; cacheStorage?: CacheStorage; plan?: BootstrapPlan } = {},
  ) {
    this.emit = emit
    this.fetcher = options.fetch ?? ((url, init) => fetch(url, init))
    this.storage = options.cacheStorage ?? globalThis.caches
    this.plan = options.plan ?? defaultPlan
  }

  handle(request: SearchRequest): Promise<void> {
    if (request.kind === "cancel") {
      if (this.staging?.generation === request.generation) this.staging.abort.abort()
      return Promise.resolve()
    }
    if (request.kind === "search") {
      if (this.active?.generation !== request.generation) return Promise.resolve()
      try {
        this.emit({
          kind: "result",
          generation: request.generation,
          queryId: request.queryId,
          page: this.active.index.search(request.query),
        })
      } catch (error) {
        this.emit({
          kind: "error",
          generation: request.generation,
          queryId: request.queryId,
          message: error instanceof Error ? error.message : "search failed",
        })
      }
      return Promise.resolve()
    }
    this.staging?.abort.abort()
    const staging = { generation: request.generation, abort: new AbortController() }
    this.staging = staging
    this.pending = this.pending.then(() => this.load(request, staging.abort.signal))
    return this.pending
  }

  private async prepare(request: LoadRequest, signal: AbortSignal) {
    const pointer = objectValue(
      parseStrict(
        await fetchBytes(
          this.fetcher,
          `${request.base}/snapshots/${request.entry === "preview" ? "preview/current.json" : "versions/index.json"}`,
          { signal, cache: "no-cache" },
        ),
      ),
    )
    let entry: JsonObject
    if (request.entry === "preview") entry = pointer
    else {
      validateIndex2(pointer)
      const current = objectValue(pointer["current"])
      const previous = pointer["previous"] === null ? null : objectValue(pointer["previous"])
      const compatible = [current, previous].find(
        (candidate) => candidate && isCompatible(candidate),
      )
      if (!compatible) throw new SnapshotError("unsupported-version", "no compatible snapshot")
      entry = compatible
    }
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
    if (request.entry === "index")
      for (const field of [
        "data_version",
        "published_at",
        "format_version",
        "min_reader_version",
        "required_capabilities",
        "engine_support_target",
      ])
        if (canonicalText(entry[field] ?? null) !== canonicalText(manifest[field] ?? null))
          throw new SnapshotError("blob-integrity", "index entry differs from manifest")
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

  private async load(request: LoadRequest, signal: AbortSignal): Promise<void> {
    const start = performance.now()
    try {
      signal.throwIfAborted()
      const {
        manifestHash,
        version,
        configKey,
        keys,
        files,
        dataVersion,
        manifestRawBytes,
        parseMs: manifestParseMs,
      } = await this.prepare(request, signal)
      let parseMs = manifestParseMs
      let mark = performance.now()
      const blobs = new SearchBlobs(request.base, this.fetcher, this.storage)
      const bootstrap = new BootstrapColumns()
      const faces = new Map<string, string>()
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
        else if (result.kind === "fragments") {
          for (const fragment of result.fragments)
            if (fragment.table === "face")
              for (const row of fragment.rows)
                faces.set(stringValue(row["id"]), stringValue(row["card_id"]))
          validatePlacement(result.fragments, version, [], faces)
        } else throw new SnapshotError("shape", "unexpected basic catalog payload")
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
      validateMediaIdentities({
        printing: bootstrap.rows("printing"),
        face: bootstrap.rows("face"),
      })
      const index = new SearchIndex(
        catalogFromIndex(bootstrap.index(), bootstrap.rows, config),
        request.edition,
      )
      buildMs += performance.now() - mark
      signal.throwIfAborted()
      this.active = { generation: request.generation, index }
      this.emit({
        kind: "ready",
        generation: request.generation,
        edition: request.edition,
        dataVersion,
        manifestHash,
        metrics: {
          files: keys.length,
          maxRawBytes,
          parseMs,
          buildMs,
          wallMs: performance.now() - start,
          firstFileMs,
          firstSetMs,
          ...index.allocation(),
        },
      })
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
