import { LRUCache } from "lru-cache"
import PQueue from "p-queue"

import { fetchBytes, type Fetcher } from "./cdn"
import { SnapshotError } from "./format-v1/errors"
import { integerValue, type JsonObject, stringValue } from "./format-v1/json"
import { transferDigest } from "./integrity"

export interface MetadataProgress {
  readonly state: "idle" | "running" | "cancelled" | "error" | "complete"
  readonly done: number
  readonly total: number
  readonly persistent: boolean
  readonly checking?: boolean
}
interface Job {
  readonly key: string
  readonly file: JsonObject
  readonly promise: Promise<Uint8Array>
  priority: boolean
  submitted: boolean
  started: boolean
  readonly gateAbort: AbortController
  readonly queueAbort: AbortController
  readonly resolve: (bytes: Uint8Array) => void
  readonly reject: (error: unknown) => void
}
const MAX_BYTES = 12 * 1024 * 1024
const MAX_FILES = 64
const cacheInitializers = new WeakMap<CacheStorage, Promise<unknown>>()

/** Bytes only: background work never decodes fragments or builds an image index. */
export class MetadataBytes {
  private readonly queue = new PQueue({ concurrency: 4 })
  private readonly background = new PQueue({ concurrency: 3 })
  private readonly pending = new Map<string, Job>()
  private readonly memory = new LRUCache<string, Uint8Array>({
    max: MAX_FILES,
    maxSize: MAX_BYTES,
    sizeCalculation: (bytes) => Math.max(1, bytes.length),
  })
  private readonly verified = new Set<string>()
  private readonly abort = new AbortController()
  private disposed = false
  private epoch = 0
  private progress: MetadataProgress
  private cache: Promise<Cache | undefined>

  private readonly base: string
  private readonly files: ReadonlyMap<string, JsonObject>
  private readonly fetcher: Fetcher
  private readonly changed: () => void
  private readonly backgroundFetch: Fetcher

  constructor(
    base: string,
    hash: string,
    files: ReadonlyMap<string, JsonObject>,
    fetcher: Fetcher,
    changed: () => void,
    storage?: CacheStorage,
    backgroundFetch: Fetcher = fetcher,
    previousHash?: string,
  ) {
    this.base = base
    this.files = files
    this.fetcher = fetcher
    this.changed = changed
    this.backgroundFetch = backgroundFetch
    this.progress = {
      state: "idle",
      done: 0,
      total: files.size,
      persistent: false,
      checking: storage !== undefined,
    }
    if (storage) {
      const scope = `${encodeURIComponent(base)}:`
      const current = `sve-images-${scope}-${hash}`
      const previous = previousHash ? `sve-images-${scope}-${previousHash}` : undefined
      const initialized = (cacheInitializers.get(storage) ?? Promise.resolve())
        .then(async () => {
          if (!this.active()) return undefined
          for (const name of await storage.keys())
            if (name.startsWith(`sve-images-${scope}-`) && name !== current && name !== previous)
              await storage.delete(name)
          if (!this.active()) return undefined
          const cache = await storage.open(current)
          if (this.active()) this.update({ persistent: true, checking: false })
          return cache
        })
        .catch(() => {
          if (this.active()) this.update({ persistent: false, checking: false })
          return undefined
        })
      cacheInitializers.set(storage, initialized)
      this.cache = initialized
    } else this.cache = Promise.resolve(undefined)
  }

  private active(): boolean {
    return !this.disposed
  }

  status(): MetadataProgress {
    return this.progress
  }

  private update(patch: Partial<MetadataProgress>): void {
    this.progress = { ...this.progress, ...patch }
    this.changed()
  }

  read(key: string, priority = true): Promise<Uint8Array> {
    if (this.disposed) return Promise.reject(new Error("snapshot replaced"))
    const memory = this.memory.get(key)
    if (memory) return Promise.resolve(memory)
    const existing = this.pending.get(key)
    if (existing) {
      if (priority && !existing.priority && !existing.started) {
        existing.priority = true
        if (existing.submitted) this.queue.setPriority(key, 1)
        else void this.submit(existing)
        // Promotion releases a background permit without duplicating its shared request.
        existing.gateAbort.abort()
      }
      return existing.promise
    }
    const file = this.files.get(key)
    if (!file) return Promise.reject(new SnapshotError("payload-set", "unknown image file"))
    const { promise, resolve, reject } = Promise.withResolvers<Uint8Array>()
    const job: Job = {
      key,
      file,
      promise,
      resolve,
      reject,
      priority,
      submitted: false,
      started: false,
      gateAbort: new AbortController(),
      queueAbort: new AbortController(),
    }
    this.pending.set(key, job)
    if (priority) void this.submit(job)
    else
      void this.background
        .add(() => this.submit(job), { signal: job.gateAbort.signal })
        .catch((error: unknown) => {
          if (!job.priority) this.rejectQueued(job, error)
        })
    return promise
  }

  private submit(job: Job): Promise<void> {
    job.submitted = true
    return this.queue
      .add(
        () => {
          job.started = true
          return this.obtain(job.key, job.file, job.priority)
        },
        { id: job.key, priority: job.priority ? 1 : 0, signal: job.queueAbort.signal },
      )
      .then(job.resolve, job.reject)
      .finally(() => {
        if (this.pending.get(job.key) === job) this.pending.delete(job.key)
      })
  }

  private rejectQueued(job: Job, error: unknown): void {
    job.reject(error)
    if (this.pending.get(job.key) === job) this.pending.delete(job.key)
  }

  private async obtain(key: string, file: JsonObject, priority: boolean): Promise<Uint8Array> {
    const path = `${this.base}/${stringValue(file["path"])}`
    const hash = stringValue(file["sha256"])
    const valid = async (bytes: Uint8Array) =>
      bytes.length === integerValue(file["bytes"]) && (await transferDigest(bytes)) === hash
    const cache = await this.cache
    if (!priority && !this.progress.persistent)
      throw new Error("persistent cache required for background fetch")
    let bytes: Uint8Array | undefined
    if (cache) {
      try {
        const response = await cache.match(path)
        if (response) {
          const cached = new Uint8Array(await response.arrayBuffer())
          if (await valid(cached)) bytes = cached
        } else if (this.verified.has(key)) this.losePersistence()
      } catch {
        this.losePersistence()
      }
    }
    if (!bytes) {
      if (!priority && !this.progress.persistent)
        throw new Error("persistent cache required for background fetch")
      for (let attempt = 0; attempt < 2; attempt += 1) {
        const candidate = await fetchBytes(priority ? this.fetcher : this.backgroundFetch, path, {
          signal: this.abort.signal,
        })
        if (await valid(candidate)) {
          bytes = candidate
          break
        }
      }
      if (!bytes)
        throw new SnapshotError("blob-integrity", "image metadata hash/length mismatch after retry")
      if (this.disposed) throw new Error("snapshot replaced")
      if (cache) {
        try {
          await cache.put(path, new Response(bytes.slice().buffer))
        } catch {
          this.losePersistence()
        }
      }
    }
    if (this.disposed) throw new Error("snapshot replaced")
    // CacheStorage owns persistent bytes; keep a bounded RAM fallback only when it fails.
    if (!this.progress.persistent && priority) this.memory.set(key, bytes)
    this.verified.add(key)
    this.update({
      done: this.verified.size,
      ...(this.verified.size === this.files.size ? { state: "complete" as const } : {}),
    })
    return bytes
  }

  private losePersistence(): void {
    this.update({ persistent: false })
    this.cancel()
  }

  async prefetch(): Promise<void> {
    await this.cache
    if (this.disposed || !this.progress.persistent) return
    this.background.start()
    const epoch = ++this.epoch
    this.update({ state: "running" })
    try {
      // Queue all keys for priority promotion, but never more than three background requests run.
      await Promise.all(
        [...this.files.keys()].map((key) => this.read(key, false).then(() => undefined)),
      )
      if (this.isCurrent(epoch)) this.update({ state: "complete" })
    } catch {
      if (this.isCurrent(epoch)) this.update({ state: "error" })
    }
  }

  private isCurrent(epoch: number): boolean {
    return !this.disposed && epoch === this.epoch
  }

  cancel(): void {
    this.background.pause()
    this.epoch += 1
    for (const job of this.pending.values()) {
      if (job.priority || job.started) continue
      const error = new Error("prefetch cancelled")
      job.gateAbort.abort(error)
      job.queueAbort.abort(error)
      this.rejectQueued(job, error)
    }
    this.update({ state: this.verified.size === this.files.size ? "complete" : "cancelled" })
  }

  dispose(): void {
    this.disposed = true
    this.queue.pause()
    this.background.pause()
    this.abort.abort()
    for (const job of this.pending.values()) {
      if (job.started) continue
      const error = new Error("snapshot replaced")
      job.gateAbort.abort(error)
      job.queueAbort.abort(error)
      this.rejectQueued(job, error)
    }
    this.memory.clear()
  }
}
