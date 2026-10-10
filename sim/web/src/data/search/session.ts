import type { Region } from "../../domain/search"
import type { SearchPage, SearchQuery } from "./index"
import type { LoadRequest, SearchEvent, SearchMetrics, SearchRequest } from "./messages"

type SearchResult =
  | { readonly phase: "pending" }
  | { readonly phase: "error"; readonly message: string }
  | {
      readonly phase: "complete"
      readonly queryId: number
      readonly generation: number
      readonly page: SearchPage
    }

export interface SearchStatus {
  readonly phase: "downloading" | "ready" | "error"
  readonly generation: number | null
  /** Region of the active generation; only queries for it are answered. */
  readonly edition?: Region
  readonly lastInput: SearchQuery | null
  readonly result: SearchResult
  readonly updating: boolean
  readonly error?: string
  readonly progress?: {
    readonly done: number
    readonly total: number
    readonly persistent: boolean
  }
  readonly dataVersion?: string
  readonly manifestHash?: string
  readonly metrics?: SearchMetrics
}
export interface SearchTransport {
  readonly send: (request: SearchRequest) => void
  readonly dispose: () => void
}
export type SearchTransportFactory = (
  receive: (event: SearchEvent) => void,
  failed: () => void,
) => SearchTransport

const browserTransport: SearchTransportFactory = (receive, failed) => {
  const worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" })
  worker.onmessage = (event: MessageEvent<SearchEvent>) => {
    receive(event.data)
  }
  worker.onerror = failed
  worker.onmessageerror = failed
  return {
    send: (request) => {
      worker.postMessage(request)
    },
    dispose: () => {
      worker.terminate()
    },
  }
}

/** Hook-ready external store: only the last input survives a download, and results are page-sized. */
export class SearchSession {
  private readonly factory: SearchTransportFactory
  /** Undefined after the Worker failed; the next load starts a new one. */
  private transport: SearchTransport | undefined
  private readonly listeners = new Set<() => void>()
  private status: SearchStatus = {
    phase: "downloading",
    generation: null,
    lastInput: null,
    result: { phase: "pending" },
    updating: false,
  }
  private nextGeneration = 0
  private staging: number | null = null
  private queryId = 0
  private loadRequest: LoadRequest | undefined
  private disposed = false

  constructor(factory: SearchTransportFactory = browserTransport) {
    this.factory = factory
    this.transport = this.connect()
  }

  /** Events of a Worker that was already replaced or failed must not touch the current status. */
  private connect(): SearchTransport {
    const transport = this.factory(
      (event) => {
        if (this.transport === transport) this.receive(event)
      },
      () => {
        if (this.transport === transport) this.failed()
      },
    )
    return transport
  }

  getStatus = (): SearchStatus => this.status

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener)
    return () => {
      this.listeners.delete(listener)
    }
  }

  private publish(status: SearchStatus): void {
    if (this.disposed) return
    this.status = status
    for (const listener of this.listeners) listener()
  }

  load(base: string, edition: Region, entry: "index" | "preview" = "index"): void {
    if (this.disposed) return
    const generation = ++this.nextGeneration
    this.staging = generation
    this.loadRequest = { kind: "load", generation, base, edition, entry }
    const { error: _error, progress: _progress, ...previous } = this.status
    const active = previous.generation !== null
    this.publish({
      ...previous,
      phase: active ? "ready" : "downloading",
      updating: active,
      // An earlier failure without an index is not the answer to the input kept for this load.
      ...(active ? {} : { result: { phase: "pending" as const } }),
    })
    // Replacing a failed Worker here rather than in its error handler means a Worker that fails
    // while starting cannot respawn in a loop.
    this.transport ??= this.connect()
    this.transport.send(this.loadRequest)
  }

  search(query: SearchQuery): void {
    if (this.disposed) return
    this.queryId += 1
    this.publish({
      ...this.status,
      lastInput: structuredClone(query),
      result:
        this.status.phase === "error"
          ? { phase: "error", message: this.status.error ?? "search unavailable" }
          : { phase: "pending" },
    })
    this.dispatch()
  }

  private dispatch(): void {
    const { generation, edition, lastInput } = this.status
    if (generation !== null && lastInput && lastInput.edition === edition)
      this.transport?.send({ kind: "search", generation, queryId: this.queryId, query: lastInput })
  }

  private receive(event: SearchEvent): void {
    if (this.disposed) return
    if (event.kind === "result" || (event.kind === "error" && event.queryId !== undefined)) {
      if (event.generation !== this.status.generation || event.queryId !== this.queryId) return
      this.publish({
        ...this.status,
        result:
          event.kind === "result"
            ? {
                phase: "complete",
                queryId: this.queryId,
                generation: event.generation,
                page: event.page,
              }
            : { phase: "error", message: event.message },
      })
      return
    }
    if (event.kind === "ready") {
      // The Worker activates every generation it finishes, even one whose ready crossed a cancel or
      // newer load, and drops queries for any other generation, so the session follows it.
      if (this.status.generation !== null && event.generation <= this.status.generation) return
      if (event.generation === this.staging) this.staging = null
      const { progress } = this.status
      this.queryId += 1
      this.publish({
        phase: "ready",
        generation: event.generation,
        edition: event.edition,
        lastInput: this.status.lastInput,
        result: { phase: "pending" },
        updating: this.staging !== null,
        ...(this.staging !== null && progress ? { progress } : {}),
        dataVersion: event.dataVersion,
        manifestHash: event.manifestHash,
        metrics: event.metrics,
      })
      this.dispatch()
      return
    }
    if (event.generation !== this.staging) return
    if (event.kind === "progress")
      this.publish({
        ...this.status,
        progress: { done: event.done, total: event.total, persistent: event.persistent },
      })
    else {
      this.staging = null
      this.publish({
        ...this.status,
        phase: this.status.generation === null ? "error" : "ready",
        updating: false,
        error: event.message,
        ...(this.status.generation === null
          ? { result: { phase: "error" as const, message: event.message } }
          : {}),
      })
    }
  }

  private failed(): void {
    this.transport?.dispose()
    this.transport = undefined
    this.staging = null
    this.queryId += 1
    this.publish({
      phase: "error",
      generation: null,
      lastInput: this.status.lastInput,
      result: { phase: "error", message: "search worker failed" },
      updating: false,
      error: "search worker failed",
    })
  }

  cancel(): void {
    if (this.staging === null || this.disposed) return
    this.transport?.send({ kind: "cancel", generation: this.staging })
    this.staging = null
    this.publish({
      ...this.status,
      phase: this.status.generation === null ? "error" : "ready",
      updating: false,
      error: "download cancelled",
      ...(this.status.generation === null
        ? { result: { phase: "error" as const, message: "download cancelled" } }
        : {}),
    })
  }

  retry(): void {
    if (!this.loadRequest || this.disposed) return
    this.load(this.loadRequest.base, this.loadRequest.edition, this.loadRequest.entry)
  }

  dispose(): void {
    this.disposed = true
    this.transport?.dispose()
    this.listeners.clear()
  }
}
