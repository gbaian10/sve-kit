import type { Region } from "../../domain/search"
import type { QueryFailure, SearchPage, SearchQuery } from "./index"
import type { LoadRequest, SearchEvent, SearchMetrics, SearchRequest } from "./messages"

type SearchResult =
  | { readonly phase: "pending" }
  | { readonly phase: "error"; readonly error: QueryFailure }
  | {
      readonly phase: "complete"
      readonly queryId: number
      readonly generation: number
      readonly root: string
      readonly page: SearchPage
    }
export interface SearchChannelStatus {
  readonly lastInput: SearchQuery | null
  readonly result: SearchResult
}
interface LoadFailure {
  readonly kind: "load-failed" | "worker-failed" | "cancelled"
  readonly message: string
}
export interface SearchStatus {
  readonly phase: "downloading" | "ready" | "error"
  readonly generation: number | null
  /** The active root and edition stay attached to old results during a replacement load. */
  readonly root?: string
  readonly edition?: Region
  readonly loading?: { readonly root: string; readonly edition: Region }
  readonly updating: boolean
  readonly error?: LoadFailure
  readonly progress?: {
    readonly done: number
    readonly total: number
    readonly persistent: boolean
  }
  readonly dataVersion?: string
  readonly manifestHash?: string
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

/** A channel has its own last input and result; the owner holds the shared Worker generation. */
export class SearchChannel {
  private readonly listeners = new Set<() => void>()
  private status: SearchChannelStatus = { lastInput: null, result: { phase: "pending" } }
  private queryId = 0
  private disposed = false

  private readonly owner: SearchSession
  readonly id: number

  constructor(owner: SearchSession, id: number) {
    this.owner = owner
    this.id = id
  }

  getStatus = (): SearchChannelStatus => this.status
  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener)
    return () => {
      this.listeners.delete(listener)
    }
  }
  private publish(status: SearchChannelStatus): void {
    if (this.disposed) return
    this.status = status
    for (const listener of this.listeners) listener()
  }
  search(query: SearchQuery): void {
    if (this.disposed) return
    this.queryId += 1
    this.publish({ lastInput: structuredClone(query), result: { phase: "pending" } })
    this.refresh(false)
  }
  refresh(invalidate = true): void {
    if (this.disposed) return
    if (invalidate) this.queryId += 1
    const status = this.owner.getStatus()
    this.publish({
      ...this.status,
      result:
        status.phase === "error"
          ? {
              phase: "error",
              error: {
                kind: "load-unavailable",
                message: status.error?.message ?? "search unavailable",
              },
            }
          : { phase: "pending" },
    })
    const query = this.status.lastInput
    this.checkAvailability()
    if (status.generation !== null && query && query.edition === status.edition)
      this.owner.send({
        kind: "search",
        generation: status.generation,
        channelId: this.id,
        queryId: this.queryId,
        query,
      })
  }
  checkAvailability(): void {
    const status = this.owner.getStatus()
    const query = this.status.lastInput
    if (
      query &&
      status.generation !== null &&
      query.edition !== status.edition &&
      status.loading?.edition !== query.edition
    )
      this.publish({
        ...this.status,
        result: {
          phase: "error",
          error: { kind: "edition-not-ready", message: "edition is not ready" },
        },
      })
  }
  receive(event: Extract<SearchEvent, { kind: "result" | "query-error" }>): void {
    const status = this.owner.getStatus()
    if (this.disposed || event.generation !== status.generation || event.queryId !== this.queryId)
      return
    this.publish({
      ...this.status,
      result:
        event.kind === "result"
          ? {
              phase: "complete",
              queryId: this.queryId,
              generation: event.generation,
              root: status.root ?? "",
              page: event.page,
            }
          : { phase: "error", error: event.error },
    })
  }
  dispose(): void {
    if (this.disposed) return
    this.disposed = true
    this.listeners.clear()
    this.owner.closeChannel(this.id)
  }
}

/** One owner serves any number of independent hook-ready query stores. */
export class SearchSession {
  private readonly listeners = new Set<() => void>()
  private readonly channels = new Map<number, SearchChannel>()
  private transport: SearchTransport | undefined
  private status: SearchStatus = { phase: "downloading", generation: null, updating: false }
  private nextGeneration = 0
  private nextChannel = 0
  private staging: number | null = null
  private loadRequest: LoadRequest | undefined
  private disposed = false

  private readonly factory: SearchTransportFactory
  private readonly onMetrics: ((metrics: SearchMetrics) => void) | undefined

  constructor(
    factory: SearchTransportFactory = browserTransport,
    onMetrics?: (metrics: SearchMetrics) => void,
  ) {
    this.factory = factory
    this.onMetrics = onMetrics
    this.transport = this.connect()
  }
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
  createChannel(): SearchChannel {
    if (this.disposed) throw new Error("search session is disposed")
    const channel = new SearchChannel(this, ++this.nextChannel)
    this.channels.set(channel.id, channel)
    return channel
  }
  send(request: SearchRequest): void {
    if (!this.disposed) this.transport?.send(request)
  }
  closeChannel(id: number): void {
    this.channels.delete(id)
    this.send({ kind: "close-channel", channelId: id })
  }
  private publish(status: SearchStatus): void {
    if (this.disposed) return
    this.status = status
    for (const listener of this.listeners) listener()
  }
  private refreshChannels(): void {
    for (const channel of this.channels.values()) channel.refresh()
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
      ...(active ? {} : { root: base }),
      loading: { root: base, edition },
    })
    if (!active) this.refreshChannels()
    // Waiting for an explicit load avoids an endless respawn when a Worker fails during startup.
    this.transport ??= this.connect()
    this.transport.send(this.loadRequest)
  }
  private receive(event: SearchEvent): void {
    if (this.disposed) return
    if (event.kind === "result" || event.kind === "query-error") {
      this.channels.get(event.channelId)?.receive(event)
      return
    }
    if (event.kind === "ready") {
      // A ready can cross a cancel or a newer load; follow the generation the Worker actually uses.
      if (this.status.generation !== null && event.generation <= this.status.generation) return
      if (event.generation === this.staging) this.staging = null
      const { progress, loading } = this.status
      this.publish({
        phase: "ready",
        generation: event.generation,
        edition: event.edition,
        root: event.root,
        updating: this.staging !== null,
        ...(this.staging !== null ? { progress, loading } : {}),
        dataVersion: event.dataVersion,
        manifestHash: event.manifestHash,
      })
      this.refreshChannels()
      this.onMetrics?.(event.metrics)
      return
    }
    if (event.generation !== this.staging) return
    if (event.kind === "progress") {
      this.publish({
        ...this.status,
        progress: { done: event.done, total: event.total, persistent: event.persistent },
      })
    } else {
      this.staging = null
      const { loading: _loading, ...previous } = this.status
      this.publish({
        ...previous,
        phase: previous.generation === null ? "error" : "ready",
        updating: false,
        error: { kind: "load-failed", message: event.message },
      })
      if (previous.generation === null) this.refreshChannels()
      else for (const channel of this.channels.values()) channel.checkAvailability()
    }
  }
  private failed(): void {
    this.transport?.dispose()
    this.transport = undefined
    this.staging = null
    this.publish({
      phase: "error",
      generation: null,
      ...(this.status.root ? { root: this.status.root } : {}),
      updating: false,
      error: { kind: "worker-failed", message: "search worker failed" },
    })
    this.refreshChannels()
  }
  cancel(): void {
    if (this.staging === null || this.disposed) return
    this.send({ kind: "cancel", generation: this.staging })
    this.staging = null
    const { loading: _loading, ...previous } = this.status
    this.publish({
      ...previous,
      phase: previous.generation === null ? "error" : "ready",
      updating: false,
      error: { kind: "cancelled", message: "download cancelled" },
    })
    if (previous.generation === null) this.refreshChannels()
    else for (const channel of this.channels.values()) channel.checkAvailability()
  }
  retry(): void {
    if (this.loadRequest && !this.disposed)
      this.load(this.loadRequest.base, this.loadRequest.edition, this.loadRequest.entry)
  }
  dispose(): void {
    this.disposed = true
    this.transport?.dispose()
    for (const channel of this.channels.values()) channel.dispose()
    this.listeners.clear()
  }
}
