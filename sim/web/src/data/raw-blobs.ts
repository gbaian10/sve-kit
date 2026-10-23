import { bodyHeaders, trimDataCaches } from "./cache-budget"
import { fetchBytes, type Fetcher } from "./cdn"
import { SnapshotError } from "./format-v3/errors"
import { integerValue, type JsonObject, stringValue } from "./format-v3/json"
import { transferDigest } from "./integrity"

export interface RawBlobDescriptor {
  readonly key: string
  readonly path: string
  readonly sha256: string
  readonly bytes: number
}

export function rawBlobDescriptor(file: JsonObject): RawBlobDescriptor {
  return {
    key: stringValue(file["key"]),
    path: stringValue(file["path"]),
    sha256: stringValue(file["sha256"]),
    bytes: integerValue(file["bytes"]),
  }
}

/** Whole retained manifests own references; a search subset must never decide raw eviction. */
export function rawRetentionPlan(
  manifests: Iterable<{ readonly hash: string; readonly files: ReadonlyMap<string, JsonObject> }>,
): ReadonlyMap<string, { readonly bytes: number; readonly manifests: ReadonlySet<string> }> {
  const references = new Map<string, { bytes: number; manifests: Set<string> }>()
  for (const manifest of manifests) {
    for (const file of manifest.files.values()) {
      const descriptor = rawBlobDescriptor(file)
      let reference = references.get(descriptor.sha256)
      if (!reference) {
        reference = { bytes: descriptor.bytes, manifests: new Set() }
        references.set(descriptor.sha256, reference)
      }
      if (reference.bytes !== descriptor.bytes)
        throw new SnapshotError("blob-integrity", "conflicting raw lengths for the same hash")
      reference.manifests.add(manifest.hash)
    }
  }
  return references
}

interface ReadOptions {
  readonly signal?: AbortSignal
  readonly priority?: "foreground" | "background" | "low" | "speculative"
}
interface Pending {
  readonly signal: AbortSignal | undefined
  readonly priority: ReadOptions["priority"]
  readonly promise: Promise<Uint8Array>
}

/** Persistent immutable bytes, with no decoded objects or settled buffers retained in RAM. */
export class RawBlobStore {
  persistent = false
  readonly ready: Promise<void>
  private failures = 0
  private readonly name: string
  private readonly inflight = new Map<string, Set<Pending>>()
  private readonly base: string
  private readonly foreground: Fetcher
  private readonly storage: CacheStorage | undefined
  private readonly background: Fetcher
  private readonly onMiss: ((file: RawBlobDescriptor) => void) | undefined

  constructor(
    base: string,
    foreground: Fetcher,
    storage?: CacheStorage,
    background: Fetcher = foreground,
    onMiss?: (file: RawBlobDescriptor) => void,
  ) {
    this.onMiss = onMiss
    this.base = base
    this.foreground = foreground
    this.storage = storage
    this.background = background
    this.name = `sve-raw:${encodeURIComponent(base)}`
    this.ready = this.open().then(() => undefined)
  }

  async read(file: RawBlobDescriptor, options: ReadOptions = {}): Promise<Uint8Array> {
    const { signal, priority = "foreground" } = options
    signal?.throwIfAborted()
    const identity = `${file.sha256}/${String(file.bytes)}`
    const pending = this.inflight.get(identity) ?? new Set<Pending>()
    // Sharing only within one cancellation owner avoids aborting an unrelated caller.
    const previous = [...pending].find((job) => job.signal === signal && job.priority === priority)
    if (previous) return previous.promise
    const job: Pending = {
      signal,
      priority,
      promise: this.obtain(file, { ...options, priority }).finally(() => {
        pending.delete(job)
        if (pending.size === 0) this.inflight.delete(identity)
      }),
    }
    pending.add(job)
    this.inflight.set(identity, pending)
    return job.promise
  }

  /** Probe verified storage without starting a download or retaining the bytes. */
  async has(file: RawBlobDescriptor): Promise<boolean> {
    const cache = await this.open()
    if (!cache) return false
    try {
      const response = await cache.match(this.slot(file))
      if (!response) return false
      const bytes = new Uint8Array(await response.arrayBuffer())
      return bytes.length === file.bytes && (await transferDigest(bytes)) === file.sha256
    } catch {
      this.failures += 1
      this.persistent = false
      return false
    }
  }

  private slot(file: RawBlobDescriptor): string {
    return `${this.base}/__sve-raw/${encodeURIComponent(file.sha256)}/bytes`
  }

  private async open(): Promise<Cache | undefined> {
    try {
      // Reopen after browser/user clearing instead of writing into a detached cache handle.
      const cache = await this.storage?.open(this.name)
      // Opening or hitting a cache cannot prove a failed write has recovered.
      if (this.failures === 0) this.persistent = cache !== undefined
      return cache
    } catch {
      this.failures += 1
      this.persistent = false
      return undefined
    }
  }

  private async obtain(file: RawBlobDescriptor, options: ReadOptions): Promise<Uint8Array> {
    const { signal, priority } = options
    signal?.throwIfAborted()
    const cache = await this.open()
    // The suffix keeps raw hashes distinct from the budget's derived-generation keys.
    const slot = this.slot(file)
    const valid = async (bytes: Uint8Array) =>
      bytes.length === file.bytes && (await transferDigest(bytes)) === file.sha256
    let cached: Uint8Array | undefined
    if (cache) {
      try {
        const response = await cache.match(slot)
        if (response) {
          const bytes = new Uint8Array(await response.arrayBuffer())
          if (await valid(bytes)) cached = bytes
          else {
            this.onMiss?.(file)
            await cache.delete(slot)
          }
        } else this.onMiss?.(file)
      } catch {
        this.failures += 1
        this.persistent = false
        // Unreadable storage is a miss, not a reason to retain a second copy in RAM.
        try {
          await cache.delete(slot)
        } catch {
          // Storage can also refuse deletion; the network still remains usable.
        }
      }
    }
    signal?.throwIfAborted()
    if (cached) return cached
    for (let attempt = 0; attempt < 2; attempt += 1) {
      const bytes = await fetchBytes(
        priority === "foreground" ? this.foreground : this.background,
        `${this.base}/${file.path}`,
        {
          ...(signal ? { signal } : {}),
          raw: { root: this.base, sha256: file.sha256, bytes: file.bytes },
          ...(priority === "speculative" ? { speculative: true } : {}),
          ...(priority === "low" ? { priority: "low" } : {}),
        },
      )
      signal?.throwIfAborted()
      if (!(await valid(bytes))) continue
      signal?.throwIfAborted()
      if (cache) await this.write(cache, slot, bytes)
      signal?.throwIfAborted()
      return bytes
    }
    throw new SnapshotError("blob-integrity", `hash/length mismatch for ${file.key} after retry`)
  }

  private async write(cache: Cache, slot: string, bytes: Uint8Array): Promise<void> {
    const failures = this.failures
    const response = () =>
      new Response(bytes.slice().buffer, {
        headers: { ...bodyHeaders(bytes.length), "content-length": String(bytes.length) },
      })
    try {
      try {
        await cache.put(slot, response())
      } catch {
        if (!this.storage) return
        await trimDataCaches(this.storage, this.base, bytes.length, true)
        await cache.put(slot, response())
      }
      if (this.storage) await trimDataCaches(this.storage, this.base)
      if (this.failures === failures) this.persistent = true
    } catch {
      this.failures += 1
      this.persistent = false
      // Quota or denied storage costs another on-demand read, never an unbounded RAM fallback.
    }
  }
}
