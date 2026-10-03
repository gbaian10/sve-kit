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
  priority: boolean
  readonly run: (priority: boolean) => Promise<Uint8Array>
  readonly resolve: (bytes: Uint8Array) => void
  readonly reject: (error: unknown) => void
}
const MAX_BYTES = 12 * 1024 * 1024
const MAX_FILES = 64
const cacheInitializers = new WeakMap<CacheStorage, Promise<unknown>>()

/** Bytes only: background work never decodes fragments or builds an image index. */
export class MetadataBytes {
  private readonly queue: Job[] = []
  private readonly pending = new Map<string, Promise<Uint8Array>>()
  private readonly memory = new Map<string, Uint8Array>()
  private readonly verified = new Set<string>()
  private readonly abort = new AbortController()
  private running = 0
  private backgroundRunning = 0
  private memoryBytes = 0
  private cancelled = false
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

  private remember(key: string, bytes: Uint8Array): void {
    const previous = this.memory.get(key)
    this.memoryBytes -= previous?.length ?? 0
    this.memory.delete(key)
    this.memory.set(key, bytes)
    this.memoryBytes += bytes.length
    while (this.memoryBytes > MAX_BYTES || this.memory.size > MAX_FILES) {
      const first = this.memory.entries().next().value
      if (!first) break
      this.memory.delete(first[0])
      this.memoryBytes -= first[1].length
    }
  }

  read(key: string, priority = true): Promise<Uint8Array> {
    if (this.disposed) return Promise.reject(new Error("snapshot replaced"))
    const memory = this.memory.get(key)
    if (memory) {
      this.remember(key, memory)
      return Promise.resolve(memory)
    }
    const existing = this.pending.get(key)
    if (existing) {
      const queued = this.queue.find((job) => job.key === key)
      if (queued) queued.priority ||= priority
      this.pump()
      return existing
    }
    const file = this.files.get(key)
    if (!file) return Promise.reject(new SnapshotError("payload-set", "unknown image file"))
    const promise = new Promise<Uint8Array>((resolve, reject) => {
      this.queue.push({
        key,
        priority,
        resolve,
        reject,
        run: (priority) => this.obtain(key, file, priority),
      })
    })
    this.pending.set(key, promise)
    this.pump()
    return promise
  }

  private pump(): void {
    while (!this.disposed && this.running < 4) {
      let index = this.queue.findIndex((job) => job.priority)
      if (index < 0) {
        if (this.cancelled || this.backgroundRunning >= 3) return
        index = 0
      }
      const job = this.queue.splice(index, 1)[0]
      if (!job) return
      this.running += 1
      if (!job.priority) this.backgroundRunning += 1
      void job
        .run(job.priority)
        .then(job.resolve, job.reject)
        .finally(() => {
          this.pending.delete(job.key)
          this.running -= 1
          if (!job.priority) this.backgroundRunning -= 1
          this.pump()
        })
    }
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
    if (!this.progress.persistent && priority) this.remember(key, bytes)
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
    this.cancelled = false
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
    this.cancelled = true
    this.epoch += 1
    const removed = this.queue.filter((job) => !job.priority)
    for (const job of removed) {
      this.queue.splice(this.queue.indexOf(job), 1)
      this.pending.delete(job.key)
      job.reject(new Error("prefetch cancelled"))
    }
    this.update({ state: this.verified.size === this.files.size ? "complete" : "cancelled" })
  }

  dispose(): void {
    this.disposed = true
    this.abort.abort()
    for (const job of this.queue.splice(0)) job.reject(new Error("snapshot replaced"))
    this.memory.clear()
    this.memoryBytes = 0
  }
}
