import { DEFAULT_QUERY } from "../../domain/query/model"
import type { Region } from "../../domain/search"
import { catalogWork } from "../catalog-timing"
import { fetchBytes, type Fetcher } from "../cdn"
import { decodeMessage } from "../format-v3/decode-message"
import { SnapshotError } from "../format-v3/errors"
import {
  arrayValue,
  canonicalText,
  integerValue,
  type JsonObject,
  objectValue,
  parseJson,
  stringValue,
} from "../format-v3/json"
import { catalogKeys, isCompatible } from "../format-v3/reader"
import { checkEntry, entryFields, indexEntries } from "../index-entry"
import { transferDigest } from "../integrity"
import { VerifiedCache } from "../verified-cache"
import { SearchBlobs } from "./blobs"
import { BootstrapColumns } from "./bootstrap"
import { EffectIndex, effectTerm } from "./effects"
import { pack, unpack } from "./image"
import { INDEX_IMAGE_FORMAT, SearchIndex, SearchQueryError } from "./index"
import type {
  BootstrapPlan,
  LoadRequest,
  QueryRequest,
  SearchEvent,
  SearchMetrics,
  SearchRequest,
} from "./messages"
import { PreviewReader } from "./preview"
import { completeImages, type CompleteView, completeView, ViewCache, viewSlot } from "./view"

const defaultPlan: BootstrapPlan = (_manifest, files, region) => catalogKeys(files, region)
/** A stored index with what the ready event and the index entry check need from its manifest. */
function readRecord(bytes: Uint8Array, edition: Region) {
  const { header, arrays } = unpack(bytes)
  const record = objectValue(header)
  const [image] = arrays
  if (arrays.length !== 1 || !(image instanceof Uint8Array))
    throw new Error("stored search record has the wrong layout")
  return {
    index: new SearchIndex(image, edition),
    dataVersion: stringValue(record["dataVersion"]),
    manifestFields: objectValue(record["manifestFields"]),
    files: integerValue(record["files"]),
    mediaFiles: new Map(
      arrayValue(record["mediaFiles"]).map((value) => {
        const file = objectValue(value)
        return [stringValue(file["key"]), file] as const
      }),
    ),
    version: stringValue(record["version"]),
  }
}

interface CachedView {
  readonly view: CompleteView
  readonly stored: Promise<void>
}

/** Downloads share the app queue; decoding follows manifest order; a replacement load aborts and drains the previous staging job. */
export class SearchEngine {
  private readonly emit: (event: SearchEvent) => void
  private readonly fetcher: Fetcher
  private readonly backgroundFetch: Fetcher
  private readonly storage: CacheStorage | undefined
  private readonly plan: BootstrapPlan
  private readonly buildId: string | undefined
  private staging: { generation: number; abort: AbortController } | undefined
  private active:
    | {
        generation: number
        effects?: Promise<EffectIndex>
        printings?: Promise<void>
        previews?: Promise<PreviewReader>
        index?: SearchIndex
        request: LoadRequest
        hash: string
        dataVersion: string
        manifestFields: JsonObject
        files: Map<string, JsonObject>
        version: string
        blobs: SearchBlobs
        views: ViewCache
        memory: Map<string, CachedView>
        building: Map<string, Promise<CachedView | undefined>>
        signal: AbortSignal
        controller: AbortController
      }
    | undefined
  private revision = -1
  private signature = ""
  private readonly queries = new Map<
    number,
    { request: QueryRequest; settled: () => void; running?: boolean }
  >()
  private rememberedRequest: QueryRequest | undefined
  private queryTimer: ReturnType<typeof setTimeout> | undefined
  private pending: Promise<void> = Promise.resolve()

  constructor(
    emit: (event: SearchEvent) => void,
    options: {
      fetch?: Fetcher
      backgroundFetch?: Fetcher
      cacheStorage?: CacheStorage
      plan?: BootstrapPlan
      buildId?: string
    } = {},
  ) {
    this.emit = (event) => {
      emit({ ...event, sentAt: performance.timeOrigin + performance.now() })
    }
    this.fetcher = options.fetch ?? ((url, init) => fetch(url, init))
    this.backgroundFetch = options.backgroundFetch ?? this.fetcher
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
    if (
      request.kind === "search" ||
      request.kind === "view" ||
      request.kind === "suggest" ||
      request.kind === "recent" ||
      request.kind === "preview" ||
      request.kind === "neighbours" ||
      request.kind === "releases" ||
      request.kind === "count" ||
      request.kind === "images"
    ) {
      if (this.active?.generation !== request.generation) return Promise.resolve()
      const previous = this.queries.get(request.channelId)
      if (previous && previous.request.queryId >= request.queryId) return Promise.resolve()
      previous?.settled()
      if (request.kind === "view" && request.remember) this.rememberedRequest = request
      const result = new Promise<void>((settled) =>
        this.queries.set(request.channelId, { request, settled }),
      )
      // A task boundary admits queued messages before scanning; each channel keeps its latest input.
      this.queryTimer ??= setTimeout(() => {
        void this.flushQueries()
      }, 0)
      return result
    }
    this.staging?.abort.abort()
    const staging = { generation: request.generation, abort: new AbortController() }
    this.staging = staging
    this.pending = this.pending.then(() => this.load(request, staging.abort.signal))
    return this.pending
  }

  private async flushQueries(): Promise<void> {
    this.queryTimer = undefined
    const active = this.active
    if (!active) return
    await Promise.all(
      [...this.queries].map(async ([channel, job]) => {
        if (job.running) return
        job.running = true
        const { request, settled } = job
        if (request.generation !== active.generation) {
          this.queries.delete(channel)
          settled()
          return
        }
        if (!active.index && request.kind !== "view") {
          job.running = false
          return
        }
        try {
          if (
            (request.kind === "search" ||
              request.kind === "view" ||
              request.kind === "neighbours" ||
              request.kind === "count") &&
            (request.query.state.rarities.length ||
              request.query.state.types.some((type) => type.startsWith("leader:")) ||
              request.query.state.altArtOnly ||
              request.query.state.unit === "art")
          )
            await active.printings
          const index = active.index
          const printingsReady = active.printings === undefined
          let eligible: ReadonlySet<string> | undefined
          if (index && ("query" in request || request.kind === "suggest")) {
            const term = effectTerm(
              request.kind === "suggest" ? request.text : request.query.state.text,
            )
            if (term !== undefined) {
              active.effects ??= EffectIndex.build(
                active.files,
                active.request.edition,
                active.version,
                active.blobs,
                index,
                active.signal,
                (done, total) => {
                  if (this.active === active && this.queries.get(channel) === job)
                    this.emit({
                      kind: "query-progress",
                      generation: request.generation,
                      channelId: channel,
                      queryId: request.queryId,
                      done,
                      total,
                    })
                },
              ).catch((error: unknown) => {
                delete active.effects
                throw error
              })
              eligible = (await active.effects).matching(term)
            }
          }
          const identity = {
            generation: request.generation,
            channelId: channel,
            queryId: request.queryId,
          }
          let event: SearchEvent
          if (request.kind === "view") {
            const key = viewSlot(request.query)
            let cached = active.memory.get(key)
            if (!cached) {
              // Channels asking for one page at once (the list and its options) share one build.
              let building = active.building.get(key)
              if (!building) {
                const { query } = request
                building = (async () => {
                  const view = await active.views.read(query, active.hash)
                  if (view) return { view, stored: Promise.resolve() }
                  if (!index) return undefined
                  const completed = completeView(query, index, eligible)
                  return {
                    view: completed,
                    // Locator-only summaries lack non-default printing badges and must refresh.
                    stored: printingsReady
                      ? active.views.write(completed, active.hash)
                      : Promise.resolve(),
                  }
                })()
                const built = building
                const drop = () => {
                  if (active.building.get(key) === built) active.building.delete(key)
                }
                built.then(drop, drop)
                active.building.set(key, built)
              }
              cached = await building
            }
            if (!cached) return
            const { view, stored } = cached
            if (printingsReady) active.memory.set(key, cached)
            while (active.memory.size > 8)
              active.memory.delete(active.memory.keys().next().value ?? "")
            event = { kind: "view-result", ...identity, view }
            const completed = view
            void stored.then(async () => {
              if (
                printingsReady &&
                request.remember &&
                this.active === active &&
                this.rememberedRequest === request
              )
                await active.views.remember(completed.query, active.hash)
            })
          } else if (request.kind === "images" && index) {
            event = {
              kind: "images-result",
              ...identity,
              faces: await completeImages(
                active.request.base,
                request.printingIds,
                index,
                active.files,
                active.version,
                active.blobs,
                active.signal,
              ),
            }
          } else if (request.kind === "suggest" || request.kind === "recent") {
            if (!index) return
            const result =
              request.kind === "suggest"
                ? {
                    kind: "suggest-result" as const,
                    ...index.suggest(request.text, request.limit, eligible),
                  }
                : { kind: "recent-result" as const, items: index.recent(request.printingIds) }
            event = { ...result, ...identity }
          } else if (request.kind === "preview" && index) {
            active.previews ??= (async () => {
              const bytes = await fetchBytes(
                this.fetcher,
                `${active.request.base}/snapshots/manifests/${active.hash.slice(7)}.json`,
                { signal: active.signal },
              )
              if ((await transferDigest(bytes)) !== active.hash)
                throw new Error("preview manifest hash mismatch")
              const decoded = decodeMessage({ kind: "manifest", bytes })
              if (decoded.kind !== "manifest") throw new Error("preview manifest shape")
              return new PreviewReader(
                decoded.files,
                integerValue(objectValue(decoded.manifest["partitioning"])["bucket_count"]),
                active.version,
                active.blobs,
              )
            })().catch((error: unknown) => {
              delete active.previews
              throw error
            })
            event = {
              kind: "preview-result",
              ...identity,
              preview: await (
                await active.previews
              ).resolve(index, request.printingId, request.faceId, request.language, active.signal),
            }
          } else if (request.kind === "releases" && index) {
            event = { kind: "releases-result", ...identity, releases: index.releases }
          } else if (request.kind === "count" && index) {
            event = {
              kind: "count-result",
              ...identity,
              counts: index.count(request.query),
            }
          } else if (request.kind === "neighbours" && index) {
            event = {
              kind: "result",
              ...identity,
              page: index.neighbours(request.query, request.target, request.maxItems, eligible),
            }
          } else if (request.kind === "search")
            event = {
              kind: "result",
              ...identity,
              page: index
                ? index.search(request.query, eligible)
                : { total: 0, offset: 0, items: [] },
            }
          else return
          if (this.active === active && this.queries.get(channel) === job) this.emit(event)
        } catch (error) {
          if (this.active === active && this.queries.get(channel) === job)
            this.emit({
              kind: "query-error",
              generation: request.generation,
              channelId: channel,
              queryId: request.queryId,
              error:
                error instanceof SearchQueryError
                  ? error.failure
                  : {
                      kind: "query-failed",
                      message: error instanceof Error ? error.message : "query failed",
                    },
            })
        } finally {
          job.running = false
          if (
            active.index ||
            (request.kind === "view" && active.memory.has(viewSlot(request.query)))
          ) {
            if (this.queries.get(channel) === job) this.queries.delete(channel)
            settled()
          }
        }
      }),
    )
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
    const { current, previous } = indexEntries(pointer)
    const compatible = [current, previous].find((candidate) => candidate && isCompatible(candidate))
    if (!compatible) throw new SnapshotError("unsupported-version", "no compatible snapshot")
    return {
      ...compatible,
      __revision: integerValue(pointer["revision"]),
      __signature: canonicalText(pointer),
    }
  }

  private async prepare(request: LoadRequest, entry: JsonObject, signal: AbortSignal) {
    const manifestHash = stringValue(entry["manifest_sha256"])
    this.emit({ kind: "stage", generation: request.generation, step: "manifest" })
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
    if (request.entry === "index") checkEntry(entry, manifest)
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
      files,
      dataVersion: stringValue(manifest["data_version"]),
      manifestFields: entryFields(manifest),
      parseMs,
      manifestRawBytes: manifestBytes.length,
    }
  }

  private adopt(active: NonNullable<SearchEngine["active"]>): void {
    if (this.active?.controller !== active.controller) this.active?.controller.abort()
    this.active = active
  }

  private ready(
    request: LoadRequest,
    metrics: SearchMetrics,
    searchReady: boolean,
    unconfirmed = false,
  ): void {
    const active = this.active
    if (!active) return
    this.emit({
      kind: "ready",
      generation: request.generation,
      edition: request.edition,
      root: request.base,
      dataVersion: active.dataVersion,
      manifestHash: active.hash,
      searchReady,
      unconfirmed,
      metrics,
    })
    void this.flushQueries()
  }

  private async load(request: LoadRequest, signal: AbortSignal): Promise<void> {
    const start = performance.now()
    try {
      signal.throwIfAborted()
      const storage = this.plan === defaultPlan ? this.storage : undefined
      const stored = new VerifiedCache(
        request.base,
        "search-index",
        INDEX_IMAGE_FORMAT,
        storage,
        this.buildId,
      )
      const selection = new VerifiedCache(
        request.base,
        "search-selection",
        INDEX_IMAGE_FORMAT,
        storage,
        this.buildId,
      )
      const views = new ViewCache(request.base, storage, this.buildId)
      const blobs = new SearchBlobs(request.base, this.fetcher, storage)
      const startupBlobs = this.active
        ? new SearchBlobs(request.base, this.backgroundFetch, storage)
        : blobs
      const emptyMetrics: SearchMetrics = {
        restored: true,
        files: 0,
        maxRawBytes: 0,
        parseMs: 0,
        buildMs: 0,
        wallMs: 0,
        firstFileMs: 0,
        firstSetMs: 0,
        arrayBuffers: 0,
        stringBytes: 0,
        cards: 0,
        stagingArrayBuffers: 0,
        stagingStringBytes: 0,
      }
      this.emit({ kind: "stage", generation: request.generation, step: "index" })
      const entryPromise = this.resolve(request, signal)
      void entryPromise.catch(() => undefined)
      const adopted = await selection.read(`${request.entry}-${request.edition}`, "active")
      let cachedEntry: JsonObject | undefined
      const publishStored = async (entry: JsonObject, cached: boolean): Promise<boolean> => {
        const hash = stringValue(entry["manifest_sha256"])
        const fallbackQuery = (await views.readCurrent(request.edition, hash)) ?? {
          state: DEFAULT_QUERY,
          edition: request.edition,
          limit: 60,
        }
        const view = await views.read(fallbackQuery, hash)
        const controller = new AbortController()
        const context = {
          generation: request.generation,
          request,
          hash,
          dataVersion: "",
          manifestFields: {} as JsonObject,
          files: new Map<string, JsonObject>(),
          version: "",
          blobs,
          views,
          memory: new Map<string, CachedView>(),
          building: new Map<string, Promise<CachedView | undefined>>(),
          signal: controller.signal,
          controller,
        }
        if (view && cachedEntry && hash === objectValue(cachedEntry["entry"])["manifest_sha256"]) {
          signal.throwIfAborted()
          context.dataVersion = stringValue(cachedEntry["dataVersion"])
          context.manifestFields = objectValue(cachedEntry["manifestFields"])
          context.memory.set(viewSlot(fallbackQuery), { view, stored: Promise.resolve() })
          this.adopt(context)
          this.ready(request, { ...emptyMetrics, wallMs: performance.now() - start }, false, cached)
        }
        const bytes = await stored.read(request.edition, hash)
        if (!bytes) return false
        let record: ReturnType<typeof readRecord>
        const mark = performance.now()
        try {
          record = readRecord(bytes, request.edition)
        } catch {
          await stored.remove(request.edition, hash)
          return false
        }
        if (request.entry === "index") checkEntry(entry, record.manifestFields)
        signal.throwIfAborted()
        this.adopt({
          ...context,
          index: record.index,
          dataVersion: record.dataVersion,
          manifestFields: record.manifestFields,
          files: record.mediaFiles,
          version: record.version,
        })
        this.ready(
          request,
          {
            ...emptyMetrics,
            files: record.files,
            maxRawBytes: bytes.length,
            cacheReadMs: stored.lastRead.readMs,
            digestMs: stored.lastRead.digestMs,
            parseMs: performance.now() - mark,
            wallMs: performance.now() - start,
            ...record.index.allocation(),
          },
          true,
          cached,
        )
        return true
      }
      // Another edition or root is served from its own stored generation, like a cold start.
      const sameSource = () =>
        this.active?.request.base === request.base &&
        this.active.request.edition === request.edition &&
        this.active.request.entry === request.entry
      if (adopted && !sameSource()) {
        try {
          cachedEntry = objectValue(parseJson(adopted))
          // Each edition keeps its own selection; an older one must not lower the revision floor.
          const revision = integerValue(cachedEntry["revision"])
          if (revision > this.revision) {
            this.revision = revision
            this.signature = stringValue(cachedEntry["signature"])
          }
          await publishStored(objectValue(cachedEntry["entry"]), true)
        } catch {
          cachedEntry = undefined
        }
      }
      const entry = await entryPromise
      if (request.entry === "index") {
        const revision = integerValue(entry["__revision"])
        const signature = stringValue(entry["__signature"])
        if (
          revision < this.revision ||
          (revision === this.revision && signature !== this.signature)
        )
          throw new SnapshotError("blob-integrity", "version index changed without a new revision")
        this.revision = revision
        this.signature = signature
      }
      // A new load aborts the previous background download, so only a complete index carries over.
      if (
        this.active?.index &&
        !this.active.printings &&
        sameSource() &&
        this.active.hash === stringValue(entry["manifest_sha256"])
      ) {
        if (request.entry === "index") checkEntry(entry, this.active.manifestFields)
        signal.throwIfAborted()
        if (this.active.generation === request.generation)
          this.emit({ kind: "checking", generation: request.generation })
        else {
          // A later check of an unchanged version carries the index over instead of rebuilding.
          this.adopt({ ...this.active, generation: request.generation, request })
          this.ready(request, { ...emptyMetrics, wallMs: performance.now() - start }, true)
        }
        return
      }
      if (!sameSource() && (await publishStored(entry, false))) return
      const {
        manifestHash,
        version,
        configKey,
        keys,
        files,
        dataVersion,
        manifestFields,
        manifestRawBytes,
        parseMs: manifestParseMs,
      } = await this.prepare(request, entry, signal)
      let parseMs = manifestParseMs
      let mark = performance.now()
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
            .map((count) => ({ kind: arrayValue(count)[1], id: arrayValue(count)[2] }))
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
        total: keys.length - 1,
        persistent: blobs.persistent,
      })
      this.emit({ kind: "stage", generation: request.generation, step: "config" })
      let done = 0
      const fetched = keys.map(async (key) => {
        const file = files.get(key)
        if (!file) throw new Error("unknown bootstrap key")
        const bytes = await startupBlobs.read(file, signal)
        if (key !== configKey) {
          done += 1
          if (!signal.aborted)
            this.emit({
              kind: "progress",
              generation: request.generation,
              done,
              total: keys.length - 1,
              persistent: blobs.persistent,
            })
        }
        if (done === keys.length - 1 && !signal.aborted)
          this.emit({ kind: "stage", generation: request.generation, step: "search" })
        return bytes
      })
      this.emit({ kind: "stage", generation: request.generation, step: "bootstrap" })
      for (const pending of fetched) void pending.catch(() => undefined)
      for (const [ordinal, key] of keys.entries()) {
        signal.throwIfAborted()
        const file = files.get(key)
        if (!file) throw new SnapshotError("payload-set", "load plan references unknown file")
        const promise = fetched[ordinal]
        if (!promise) throw new Error("bootstrap request missing")
        const bytes = await promise
        maxRawBytes = Math.max(maxRawBytes, bytes.length)
        mark = performance.now()
        const result = decodeMessage({ kind: "file", bytes, file, version })
        if (key === configKey && result.kind === "config") config = result.value
        else if (result.kind !== "fragments")
          throw new SnapshotError("shape", "unexpected basic catalog payload")
        parseMs += performance.now() - mark
        mark = performance.now()
        if (result.kind === "fragments")
          catalogWork("ingest", () => bootstrap?.ingest(result.fragments))
        buildMs += performance.now() - mark
        if (ordinal === 0) firstFileMs = performance.now() - start
        for (const owner of ownersOf(file)) {
          const remaining = (remainingSets.get(owner) ?? 1) - 1
          remainingSets.set(owner, remaining)
          if (!remaining && !firstSetMs) firstSetMs = performance.now() - start
        }
      }
      signal.throwIfAborted()
      if (!config) throw new SnapshotError("config-programs-count", "config missing")
      mark = performance.now()
      this.emit({ kind: "stage", generation: request.generation, step: "search" })
      catalogWork("seal", () => bootstrap?.seal())
      const stagingAllocation = bootstrap.allocation()
      const sealedBootstrap = bootstrap
      const index = catalogWork(
        "SearchIndex",
        () => new SearchIndex(sealedBootstrap, request.edition),
      )
      buildMs += performance.now() - mark
      signal.throwIfAborted()
      const background = bootstrap
      bootstrap = undefined
      const mediaFiles = new Map(
        [...files].filter(([, file]) =>
          arrayValue(file["row_counts"]).some((count) => arrayValue(count)[0] === "printing_image"),
        ),
      )
      for (const file of [...mediaFiles.values()]) {
        for (const raw of arrayValue(file["dependencies"])) {
          const ref = objectValue(raw)
          const key = stringValue(ref["key"])
          const dependency = files.get(key)
          if (!dependency || dependency["sha256"] !== ref["sha256"])
            throw new SnapshotError("shape", "media dependency")
          mediaFiles.set(key, dependency)
        }
      }
      const printingsGate = Promise.withResolvers<undefined>()
      void printingsGate.promise.catch(() => undefined)
      const controller = new AbortController()
      const active: NonNullable<SearchEngine["active"]> = {
        printings: printingsGate.promise,
        generation: request.generation,
        index,
        request,
        hash: manifestHash,
        dataVersion,
        manifestFields,
        files,
        version,
        blobs,
        views,
        memory: new Map(),
        building: new Map(),
        signal: controller.signal,
        controller,
      }
      this.adopt(active)
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
      void this.flushQueries()
      const backgroundBlobs = new SearchBlobs(request.base, this.backgroundFetch, this.storage)
      const backgroundFiles = [...files.values()].filter(
        (file) =>
          ["print", "product"].includes(stringValue(file["purpose"])) &&
          (file["region"] === null || file["region"] === request.edition),
      )
      let printingsApplied = false
      try {
        const parts = await Promise.all(
          backgroundFiles.map(async (file) =>
            decodeMessage({
              kind: "file",
              file,
              bytes: await backgroundBlobs.read(file, signal),
              version,
            }),
          ),
        )
        signal.throwIfAborted()
        if (this.active === active) {
          for (const part of parts)
            if (part.kind === "fragments")
              catalogWork("background:ingest", () => {
                background.ingest(part.fragments)
              })
          catalogWork("background:seal", () => {
            background.seal()
          })
          catalogWork("background:SearchIndex", () => {
            index.completePrintings(background)
          })
          active.memory.clear()
          active.building.clear()
          printingsApplied = true
          printingsGate.resolve(undefined)
          // Like a restored image, the context is now complete and an unchanged check may keep it.
          delete active.printings
          this.emit({ kind: "background-ready", generation: request.generation })
        }
      } catch (error) {
        printingsGate.reject(error)
        signal.throwIfAborted()
      }
      // A restored image is never supplemented again, so a catalog-only index is rebuilt next time.
      if (!printingsApplied) return
      await blobs.retainPlan(
        request.edition,
        [
          ...new Set([
            ...keys.flatMap((key) => {
              const file = files.get(key)
              return file ? [file] : []
            }),
            ...mediaFiles.values(),
            ...backgroundFiles,
          ]),
        ],
        manifestHash,
      )
      // Written only now: every file passed its checks and this generation is already published.
      await stored.write(
        request.edition,
        manifestHash,
        pack(
          {
            dataVersion,
            manifestFields,
            files: keys.length,
            mediaFiles: [...files.values()],
            version,
          },
          [index.encode()],
        ),
      )
      signal.throwIfAborted()
      if (this.active?.generation === request.generation)
        await selection.write(
          `${request.entry}-${request.edition}`,
          "active",
          new TextEncoder().encode(
            JSON.stringify({
              entry,
              dataVersion,
              manifestFields,
              revision: this.revision,
              signature: this.signature,
            }),
          ),
        )
    } catch (error) {
      if (!signal.aborted && this.active?.generation === request.generation)
        this.emit({
          kind: "checking",
          generation: request.generation,
          error: error instanceof Error ? error.message : "update check failed",
        })
      else if (!signal.aborted)
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
