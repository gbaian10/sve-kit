import type { Fetcher } from "./cdn"
import { FetchPlan, type FetchProgress, type FetchSelection } from "./fetch-plan"
import { RawBlobStore } from "./raw-blobs"
import { appRequests } from "./request-queue"

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
    }
  }

  getStatus = (): DownloadStatus => this.status
  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener)
    return () => {
      this.listeners.delete(listener)
    }
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
    this.plan = selection
      ? new FetchPlan(
          selection,
          new RawBlobStore(
            selection.base,
            this.options.foreground ?? appRequests.foreground,
            this.options.storage ?? globalThis.caches,
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

  private publish(status: DownloadStatus): void {
    this.status = status
    for (const listener of this.listeners) listener()
  }

  private cancel(): void {
    this.run?.abort()
    this.run = undefined
  }

  private reconcile = (): void => {
    if (this.disposed || !this.plan) return
    const paused =
      (typeof document !== "undefined" && document.hidden) ||
      (typeof navigator !== "undefined" && !navigator.onLine)
    if (!this.enabled || paused) {
      this.cancel()
      this.publish({ ...this.status, state: this.enabled ? "paused" : "disabled" })
      return
    }
    if (this.run || this.status.state === "settled") return
    const plan = this.plan
    const run = new AbortController()
    this.run = run
    this.publish({ ...this.status, state: "running" })
    void plan
      .run(run.signal, (progress) => {
        if (this.run !== run || run.signal.aborted || this.plan !== plan) return
        this.publish({ selection: plan.selection, state: "running", progress })
      })
      .then(
        () => {
          if (this.run !== run || run.signal.aborted) return
          this.run = undefined
          this.publish({ ...this.status, state: "settled" })
        },
        () => {
          if (this.run !== run || run.signal.aborted) return
          this.run = undefined
          this.publish({ ...this.status, state: "settled" })
        },
      )
  }

  dispose(): void {
    this.disposed = true
    this.cancel()
    if (typeof document !== "undefined")
      document.removeEventListener("visibilitychange", this.reconcile)
    if (typeof window !== "undefined") {
      window.removeEventListener("online", this.reconcile)
      window.removeEventListener("offline", this.reconcile)
    }
    this.listeners.clear()
  }
}
