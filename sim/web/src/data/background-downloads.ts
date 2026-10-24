import { readDataCacheBudget } from "./cache-budget"
import type { Fetcher } from "./cdn"
import { FetchPlan, type FetchProgress, type FetchSelection } from "./fetch-plan"
import { decodeMessage } from "./format-v3/decode-message"
import { stringValue } from "./format-v3/json"
import { transferDigest } from "./integrity"
import { RawBlobStore } from "./raw-blobs"
import { appRequests } from "./request-queue"
import { retainManifest } from "./retained-manifests"

interface AdoptedManifest {
  readonly base: string
  readonly manifestHash: string
  readonly manifestPath: string
  readonly dataVersion: string
  readonly edition: FetchSelection["edition"]
}

interface DownloadStatus {
  readonly state: "idle" | "disabled" | "paused" | "running" | "settled"
  readonly selection?: Omit<FetchSelection, "files">
  readonly progress?: FetchProgress
}

/** One selected byte plan owns its cancellation and browser lifecycle. */
export class BackgroundDownloads {
  private readonly listeners = new Set<() => void>()
  private readonly options: {
    readonly storage?: CacheStorage
    readonly foreground?: Fetcher
    readonly background?: Fetcher
  }
  private enabled: boolean
  private plan: FetchPlan | undefined
  private run: AbortController | undefined
  private adoption: AbortController | undefined
  private adopted: AdoptedManifest | undefined
  private selected: FetchSelection | undefined
  private preferredEdition: FetchSelection["edition"] | undefined
  private disposed = false
  private status: DownloadStatus = { state: "idle" }

  constructor(
    options: {
      readonly enabled?: boolean
      readonly storage?: CacheStorage
      readonly foreground?: Fetcher
      readonly background?: Fetcher
    } = {},
  ) {
    this.options = options
    this.enabled = options.enabled ?? false
    if (typeof document !== "undefined")
      document.addEventListener("visibilitychange", this.reconcile)
    if (typeof window !== "undefined") {
      window.addEventListener("online", this.reconcile)
      window.addEventListener("offline", this.reconcile)
      window.addEventListener("sve-cache-cleared", this.cleared)
    }
  }

  getStatus = (): DownloadStatus => this.status
  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener)
    return () => {
      this.listeners.delete(listener)
    }
  }

  adopt(selection: AdoptedManifest | null): void {
    if (this.disposed) return
    if (selection && JSON.stringify(selection) === JSON.stringify(this.adopted)) return
    this.adoption?.abort()
    this.adoption = undefined
    this.adopted = selection ?? undefined
    this.preferredEdition = selection?.edition
    this.cancel()
    this.selected = undefined
    this.plan = undefined
    this.publish({ state: this.enabled ? "idle" : "disabled", ...(selection ? { selection } : {}) })
    this.reconcile()
  }

  private prepare(selection: AdoptedManifest): void {
    const abort = new AbortController()
    this.adoption = abort
    this.publish({ state: this.enabled ? "running" : "disabled", selection })
    // The exact adopted reference also works for a restored search record with only media files.
    void (async () => {
      const response = await (this.options.foreground ?? appRequests.foreground)(
        `${selection.base}/${selection.manifestPath}`,
        { signal: abort.signal },
      )
      if (!response.ok) throw new Error("manifest unavailable")
      const bytes = new Uint8Array(await response.arrayBuffer())
      if ((await transferDigest(bytes)) !== selection.manifestHash)
        throw new Error("manifest hash mismatch")
      const decoded = decodeMessage({ kind: "manifest", bytes })
      if (
        decoded.kind !== "manifest" ||
        stringValue(decoded.manifest["data_version"]) !== selection.dataVersion
      )
        throw new Error("adopted manifest differs")
      const length = response.headers.get("content-length")
      const brotli =
        response.headers.get("content-encoding") === "br" &&
        length !== null &&
        length.trim() !== "" &&
        Number.isSafeInteger(Number(length)) &&
        Number(length) >= 0
          ? Number(length)
          : null
      if (abort.signal.aborted || this.adoption !== abort) return
      this.select({
        ...selection,
        edition: this.preferredEdition ?? selection.edition,
        files: decoded.files,
        manifest: {
          key: "manifest",
          path: selection.manifestPath,
          sha256: selection.manifestHash,
          bytes: bytes.length,
          brotli,
        },
      })
    })().catch(() => {
      if (abort.signal.aborted || this.adoption !== abort) return
      this.publish({ state: "settled", selection })
    })
  }

  select(selection: FetchSelection | null): void {
    if (this.disposed) return
    const previous = this.plan?.selection
    if (
      previous &&
      selection &&
      previous.base === selection.base &&
      previous.manifestHash === selection.manifestHash &&
      previous.dataVersion === selection.dataVersion &&
      previous.edition === selection.edition
    )
      return
    this.cancel()
    this.selected = selection ?? undefined
    this.plan = selection
      ? new FetchPlan(
          selection,
          new RawBlobStore(
            selection.base,
            this.options.foreground ?? appRequests.foreground,
            this.options.storage ?? (typeof caches === "undefined" ? undefined : caches),
            this.options.background ?? appRequests.background,
          ),
        )
      : undefined
    this.publish({ state: "idle", ...(this.plan ? { selection: this.plan.selection } : {}) })
    this.reconcile()
  }

  setEnabled(enabled: boolean): void {
    if (this.disposed || this.enabled === enabled) return
    this.enabled = enabled
    this.reconcile()
  }

  preferEdition(base: string, edition: FetchSelection["edition"]): void {
    if (this.disposed || this.adopted?.base !== base) return
    this.preferredEdition = edition
    if (this.selected) this.select({ ...this.selected, edition })
  }

  async recheck(): Promise<void> {
    const plan = this.plan
    if (!plan || this.run || this.status.progress?.phase !== "complete") return
    const run = new AbortController()
    this.run = run
    try {
      const progress = await plan.inspect(run.signal)
      if (this.run !== run || run.signal.aborted) return
      this.publish({
        ...this.status,
        progress: {
          ...progress,
          phase: progress.missing === 0 ? "complete" : "partial",
          ...(progress.missing ? { reason: "cleared" as const } : {}),
        },
      })
    } catch {
      if (this.run === run && !run.signal.aborted) this.cleared()
    } finally {
      if (this.run === run) this.run = undefined
    }
  }

  private cleared = (): void => {
    if (!this.plan && !this.adopted) return
    this.cancel()
    const progress = this.status.progress
    this.publish({
      ...this.status,
      state: this.enabled ? "settled" : "disabled",
      ...(progress
        ? {
            progress: {
              ...progress,
              phase: "partial",
              reason: "cleared",
              missing: progress.total,
              missingRawBytes: progress.rawBytes,
              missingBrotliBytes: progress.brotliBytes,
            },
          }
        : {}),
    })
  }

  private publish(status: DownloadStatus): void {
    this.status = status
    for (const listener of this.listeners) listener()
  }

  private cancel(): void {
    this.run?.abort()
    this.run = undefined
  }

  private reconcile = (): void => {
    if (this.disposed) return
    if (!this.plan && !this.adopted) return
    const paused =
      (typeof document !== "undefined" && document.hidden) ||
      (typeof navigator !== "undefined" && !navigator.onLine)
    if (!this.enabled || paused) {
      this.cancel()
      this.adoption?.abort()
      this.adoption = undefined
      this.publish({ ...this.status, state: this.enabled ? "paused" : "disabled" })
      return
    }
    if (!this.plan) {
      if (this.adopted && !this.adoption && this.status.state !== "settled")
        this.prepare(this.adopted)
      return
    }
    if (this.run || this.status.state === "settled") return
    const plan = this.plan
    const run = new AbortController()
    this.run = run
    this.publish({ ...this.status, state: "running" })
    void (async () => {
      const inspected = await plan.inspect(run.signal)
      if (this.run !== run || run.signal.aborted) return
      this.publish({ selection: plan.selection, state: "running", progress: inspected })
      const reject = (reason: NonNullable<FetchProgress["reason"]>) => {
        this.publish({
          selection: plan.selection,
          state: "running",
          progress: {
            ...inspected,
            phase: "partial",
            reason,
          },
        })
      }
      if (inspected.brotliBytes === null || inspected.brotliBytes > 12 * 1024 * 1024) {
        reject("batch-limit")
        return
      }
      const storage = this.options.storage ?? (typeof caches === "undefined" ? undefined : caches)
      if (!storage) {
        reject("storage")
        return
      }
      const files = this.selected?.files
      if (!files || this.plan !== plan) return
      await retainManifest(storage, plan.selection.base, plan.selection.manifestHash, files)
      const budget = await readDataCacheBudget(storage, plan.selection.base)
      await budget.trim(0, false, true)
      run.signal.throwIfAborted()
      if (!budget.fits(plan.blobs)) {
        reject("capacity")
        return
      }
      await plan.run(
        run.signal,
        (progress) => {
          if (this.run !== run || run.signal.aborted || this.plan !== plan) return
          this.publish({ selection: plan.selection, state: "running", progress })
        },
        budget,
      )
    })().then(
      () => {
        if (this.run !== run || run.signal.aborted) return
        this.run = undefined
        this.publish({ ...this.status, state: "settled" })
      },
      () => {
        if (this.run !== run || run.signal.aborted) return
        this.run = undefined
        this.publish({
          ...this.status,
          state: "settled",
          ...(this.status.progress
            ? { progress: { ...this.status.progress, phase: "partial", reason: "storage" } }
            : {}),
        })
      },
    )
  }

  dispose(): void {
    this.disposed = true
    this.cancel()
    this.adoption?.abort()
    if (typeof document !== "undefined")
      document.removeEventListener("visibilitychange", this.reconcile)
    if (typeof window !== "undefined") {
      window.removeEventListener("online", this.reconcile)
      window.removeEventListener("offline", this.reconcile)
      window.removeEventListener("sve-cache-cleared", this.cleared)
    }
    this.listeners.clear()
  }
}

export const appBackgroundDownloads = new BackgroundDownloads({ enabled: true })
