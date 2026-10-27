import PQueue from "p-queue"

import type { Fetcher } from "./cdn"
import { SnapshotError } from "./format-v3/errors"
import type { JsonObject } from "./format-v3/json"
import { rawBlobDescriptor, RawBlobStore } from "./raw-blobs"

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
const cacheInitializers = new WeakMap<CacheStorage, Promise<unknown>>()

/** Bytes only: background work never decodes fragments or builds an image index. */
export class MetadataBytes {
  private readonly queue = new PQueue({ concurrency: 4 })
  private readonly background = new PQueue({ concurrency: 3 })
  private readonly pending = new Map<string, Job>()
  private readonly verified = new Set<string>()
  private readonly abort = new AbortController()
  private disposed = false
  private readonly initialized: Promise<void>
  private readonly raw: RawBlobStore

  private readonly files: ReadonlyMap<string, JsonObject>

  constructor(
    base: string,
    files: ReadonlyMap<string, JsonObject>,
    fetcher: Fetcher,
    storage?: CacheStorage,
    backgroundFetch: Fetcher = fetcher,
  ) {
    this.files = files
    this.raw = new RawBlobStore(base, fetcher, storage, backgroundFetch, (file) => {
      this.rawMissed(file.sha256)
    })
    if (storage) {
      const scope = `${encodeURIComponent(base)}:`
      const initialized = (cacheInitializers.get(storage) ?? Promise.resolve())
        .then(async () => {
          if (!this.active()) return
          for (const name of await storage.keys())
            if (name.startsWith(`sve-images-${scope}-`)) await storage.delete(name)
        })
        .catch(() => {
          // A later session retries legacy cleanup if storage refuses it.
        })
      cacheInitializers.set(storage, initialized)
      this.initialized = Promise.all([initialized, this.raw.ready]).then(() => undefined)
    } else this.initialized = this.raw.ready
  }

  private active(): boolean {
    return !this.disposed
  }

  read(key: string, priority = true, signal?: AbortSignal): Promise<Uint8Array> {
    if (this.disposed) return Promise.reject(new Error("snapshot replaced"))
    if (signal) {
      signal = AbortSignal.any([this.abort.signal, signal])
      const file = this.files.get(key)
      if (!file) return Promise.reject(new SnapshotError("payload-set", "unknown image file"))
      return this.queue.add(() => this.obtain(key, file, priority, signal), {
        signal,
        priority: priority ? 1 : 0,
      })
    }
    const existing = this.pending.get(key)
    if (existing) {
      if (priority && !existing.priority && existing.started)
        return this.read(key, true, this.abort.signal)
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
      .finally(() => {
        if (this.pending.get(job.key) === job) this.pending.delete(job.key)
      })
      .then(job.resolve, job.reject)
  }

  private rejectQueued(job: Job, error: unknown): void {
    job.reject(error)
    if (this.pending.get(job.key) === job) this.pending.delete(job.key)
  }

  private async obtain(
    key: string,
    file: JsonObject,
    priority: boolean,
    signal?: AbortSignal,
  ): Promise<Uint8Array> {
    const activeSignal = signal ? AbortSignal.any([this.abort.signal, signal]) : this.abort.signal
    if (!this.active()) throw new Error("snapshot replaced")
    activeSignal.throwIfAborted()
    await this.initialized
    if (!priority && !this.raw.persistent)
      throw new Error("persistent cache required for background fetch")
    let bytes: Uint8Array
    try {
      bytes = await this.raw.read(rawBlobDescriptor(file), {
        signal: activeSignal,
        priority: priority ? "foreground" : signal ? "low" : "background",
      })
    } catch (error) {
      if (this.disposed) throw new Error("snapshot replaced", { cause: error })
      throw error
    }
    if (this.disposed) throw new Error("snapshot replaced")
    activeSignal.throwIfAborted()
    if (!this.raw.persistent) {
      // Without usable storage there is nothing verified to lose; only a real downgrade signals it.
      if (this.verified.size > 0) this.losePersistence()
      return bytes
    }
    this.verified.add(key)
    return bytes
  }

  /** A verified file vanished from storage, so the other verified claims can no longer be trusted. */
  private rawMissed(sha256: string): void {
    if (this.disposed) return
    for (const key of this.verified) {
      if (this.files.get(key)?.["sha256"] !== sha256) continue
      this.verified.clear()
      // Stop background refill; a foreground read re-adds only the file it actually restores.
      this.cancel()
      return
    }
  }

  private losePersistence(): void {
    this.verified.clear()
    this.cancel()
  }

  cancel(): void {
    for (const job of this.pending.values()) {
      if (job.priority || job.started) continue
      const error = new Error("background metadata read cancelled")
      job.gateAbort.abort(error)
      job.queueAbort.abort(error)
      this.rejectQueued(job, error)
    }
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
  }
}
