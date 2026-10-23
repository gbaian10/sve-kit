import type { Region } from "../domain/search"
import { SnapshotError } from "./format-v3/errors"
import {
  arrayValue,
  integerValue,
  type JsonObject,
  objectValue,
  stringValue,
} from "./format-v3/json"
import { type RawBlobDescriptor, rawBlobDescriptor, RawBlobStore } from "./raw-blobs"
import { appRequests } from "./request-queue"

export interface FetchSelection {
  readonly base: string
  readonly manifestHash: string
  readonly dataVersion: string
  readonly edition: Region
  /** Files from the adopted, verified manifest, including all regions. */
  readonly files: ReadonlyMap<string, JsonObject>
  readonly manifest?: RawBlobDescriptor & { readonly brotli: number | null }
}

interface PlannedBlob extends RawBlobDescriptor {
  readonly brotli: number | null
}

export interface FetchProgress {
  readonly phase: "checking" | "downloading" | "complete" | "partial"
  readonly reason?: "batch-limit" | "capacity" | "storage" | "write" | "cleared" | "network"
  readonly total: number
  readonly rawBytes: number
  readonly brotliBytes: number | null
  readonly missing: number
  readonly missingRawBytes: number
  readonly missingBrotliBytes: number | null
}

/** A fixed byte plan; no downloaded container is decoded or indexed. */
export class FetchPlan {
  readonly selection: Omit<FetchSelection, "files">
  readonly blobs: readonly PlannedBlob[]
  private readonly raw: RawBlobStore

  constructor(selection: FetchSelection, raw?: RawBlobStore) {
    const { files, manifest, ...identity } = selection
    this.selection = Object.freeze(identity)
    this.raw =
      raw ??
      new RawBlobStore(
        selection.base,
        appRequests.foreground,
        globalThis.caches,
        appRequests.background,
      )
    const selected = new Set<string>()
    const add = (key: string, hash?: string) => {
      const file = files.get(key)
      if (!file || (hash !== undefined && file["sha256"] !== hash))
        throw new SnapshotError("shape", "fetch plan reference mismatch")
      selected.add(key)
      return file
    }
    const references = (file: JsonObject, field: string) =>
      arrayValue(file[field]).map((raw) => {
        const ref = objectValue(raw)
        return { key: stringValue(ref["key"]), hash: stringValue(ref["sha256"]) }
      })
    const dependencies = () => {
      const pending = [...selected]
      while (pending.length) {
        const file = files.get(pending.pop() ?? "")
        if (!file) throw new SnapshotError("shape", "fetch plan dependency missing")
        for (const ref of references(file, "dependencies")) {
          const seen = selected.has(ref.key)
          add(ref.key, ref.hash)
          if (!seen) pending.push(ref.key)
        }
      }
    }
    for (const [key, file] of files)
      if (
        ["bootstrap", "text", "config"].includes(stringValue(file["role"])) &&
        (file["region"] === null || file["region"] === selection.edition)
      )
        add(key)
    dependencies()
    // Only C0 contributes source pointers; a newly added source never contributes another hop.
    for (const key of [...selected]) {
      const file = files.get(key)
      if (file) for (const ref of references(file, "source_refs")) add(ref.key, ref.hash)
    }
    dependencies()
    for (const [key, file] of files)
      if (["images", "programs"].includes(stringValue(file["role"]))) add(key)
    const blobs = new Map<string, PlannedBlob>()
    if (manifest) blobs.set(manifest.sha256, Object.freeze({ ...manifest }))
    for (const key of selected) {
      const file = files.get(key)
      if (!file) throw new SnapshotError("shape", "fetch plan file missing")
      const descriptor = rawBlobDescriptor(file)
      const compressed = objectValue(file["compressed_bytes"])["br"]
      const blob = Object.freeze({
        ...descriptor,
        brotli: compressed === null ? null : integerValue(compressed),
      })
      const previous = blobs.get(blob.sha256)
      if (previous && (previous.bytes !== blob.bytes || previous.brotli !== blob.brotli))
        throw new SnapshotError("shape", "fetch plan hash has conflicting sizes")
      blobs.set(blob.sha256, blob)
    }
    this.blobs = Object.freeze([...blobs.values()])
  }

  // A method call keeps TypeScript from narrowing a flag that other awaits can flip.
  private stored(): boolean {
    return this.raw.persistent
  }

  private summarize(missing: readonly PlannedBlob[], phase: FetchProgress["phase"]): FetchProgress {
    const sum = (blobs: readonly PlannedBlob[], field: "bytes" | "brotli") =>
      blobs.some((blob) => blob[field] === null)
        ? null
        : blobs.reduce((total, blob) => total + (blob[field] ?? 0), 0)
    return {
      phase,
      total: this.blobs.length,
      rawBytes: sum(this.blobs, "bytes") ?? 0,
      brotliBytes: sum(this.blobs, "brotli"),
      missing: missing.length,
      missingRawBytes: sum(missing, "bytes") ?? 0,
      missingBrotliBytes: sum(missing, "brotli"),
    }
  }

  async inspect(signal: AbortSignal): Promise<FetchProgress> {
    await this.raw.ready
    const missing: PlannedBlob[] = []
    for (const blob of this.blobs) {
      signal.throwIfAborted()
      if (!(await this.raw.has(blob))) missing.push(blob)
    }
    signal.throwIfAborted()
    return this.summarize(missing, "checking")
  }

  async run(signal: AbortSignal, progress: (value: FetchProgress) => void): Promise<void> {
    const publish = (
      missing: readonly PlannedBlob[],
      phase: FetchProgress["phase"],
      reason?: FetchProgress["reason"],
    ) => {
      signal.throwIfAborted()
      progress({ ...this.summarize(missing, phase), ...(reason ? { reason } : {}) })
    }
    const missing = async () => {
      const out: PlannedBlob[] = []
      for (const blob of this.blobs) {
        signal.throwIfAborted()
        if (!(await this.raw.has(blob))) out.push(blob)
      }
      signal.throwIfAborted()
      return out
    }
    publish(this.blobs, "checking")
    await this.raw.ready
    let remaining = await missing()
    if (!this.stored()) {
      publish(remaining, "partial", "storage")
      return
    }
    publish(remaining, "downloading")
    // A small producer window avoids queuing a whole manifest of cache probes and buffers.
    const pending = [...remaining]
    let failed = false
    const didFail = () => failed
    await Promise.all(
      Array.from({ length: 2 }, async () => {
        while (!failed && this.stored()) {
          signal.throwIfAborted()
          const blob = pending.shift()
          if (!blob) return
          try {
            await this.raw.read(blob, { signal, priority: "speculative" })
            signal.throwIfAborted()
            if (!this.stored() || !(await this.raw.has(blob))) {
              failed = true
              return
            }
            remaining = remaining.filter((file) => file.sha256 !== blob.sha256)
            publish(remaining, "downloading")
          } catch {
            signal.throwIfAborted()
            failed = true
            return
          }
        }
      }),
    )
    // Eviction or clear during the batch invalidates earlier writes; never refill in a loop.
    remaining = await missing()
    const complete = this.stored() && remaining.length === 0
    publish(
      remaining,
      complete ? "complete" : "partial",
      complete ? undefined : !this.stored() ? "write" : didFail() ? "network" : "cleared",
    )
  }
}
