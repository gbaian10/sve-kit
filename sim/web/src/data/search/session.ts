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
  private transport: SearchTransport
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
  private activeEdition: Region | undefined
  private queryId = 0
  private loadRequest: LoadRequest | undefined
  private disposed = false

  constructor(factory: SearchTransportFactory = browserTransport) {
    this.factory = factory
    this.transport = factory(this.receive, this.failed)
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
    this.publish({
      ...previous,
      phase: this.status.generation === null ? "downloading" : "ready",
      updating: this.status.generation !== null,
    })
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
    const { generation, lastInput } = this.status
    if (generation !== null && lastInput && lastInput.edition === this.activeEdition)
      this.transport.send({ kind: "search", generation, queryId: this.queryId, query: lastInput })
  }

  private receive = (event: SearchEvent): void => {
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
    if (event.generation !== this.staging) return
    if (event.kind === "progress")
      this.publish({
        ...this.status,
        progress: { done: event.done, total: event.total, persistent: event.persistent },
      })
    else if (event.kind === "ready") {
      this.staging = null
      this.activeEdition = this.loadRequest?.edition
      this.queryId += 1
      this.publish({
        phase: "ready",
        generation: event.generation,
        lastInput: this.status.lastInput,
        result: { phase: "pending" },
        updating: false,
        dataVersion: event.dataVersion,
        metrics: event.metrics,
      })
      this.dispatch()
    } else {
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

  private failed = (): void => {
    this.transport.dispose()
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
    this.transport.send({ kind: "cancel", generation: this.staging })
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
    if (this.status.generation === null) {
      this.transport.dispose()
      this.transport = this.factory(this.receive, this.failed)
    }
    this.load(this.loadRequest.base, this.loadRequest.edition, this.loadRequest.entry)
  }

  dispose(): void {
    this.disposed = true
    this.transport.dispose()
    this.listeners.clear()
  }
}
