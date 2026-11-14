// Main-thread side of the local battle: one channel per player seat and the hot-seat view.
import type {
  Command,
  InitMessage,
  MatchInputs,
  MatchOptions,
  Receipt,
  Seat,
  SeatMessage,
  SeatRequest,
  Update,
} from "./protocol"
import { SEATS } from "./protocol"

export interface ChannelPort {
  postMessage(message: SeatRequest): void
  onmessage: ((event: MessageEvent<SeatMessage>) => void) | null
  close(): void
}

/** Thrown locally when a second command is sent before the first one's receipt. */
export class CommandOutstandingError extends Error {
  constructor() {
    super("a command is already outstanding on this seat")
  }
}

/** Rejects an outstanding command whose channel was closed before its receipt. */
export class ChannelClosedError extends Error {
  constructor() {
    super("the battle channel was closed")
  }
}

/** One seat's ordered channel; at most one rules command is in flight, never retried. */
export class SeatChannel {
  readonly seat: Seat
  readonly #port: ChannelPort
  readonly #listeners = new Set<(message: SeatMessage) => void>()
  #pending:
    { id: string; resolve: (receipt: Receipt) => void; reject: (error: Error) => void } | undefined
  #sent = 0
  #closed = false

  constructor(seat: Seat, port: ChannelPort) {
    this.seat = seat
    this.#port = port
    port.onmessage = (event) => {
      this.#receive(event.data)
    }
  }

  get busy(): boolean {
    return this.#pending !== undefined
  }

  /** Every non-receipt message, in delivery order. */
  listen(listener: (message: SeatMessage) => void): () => void {
    this.#listeners.add(listener)
    return () => this.#listeners.delete(listener)
  }

  submit(command: Command): Promise<Receipt> {
    if (this.#closed) return Promise.reject(new ChannelClosedError())
    if (this.#pending !== undefined) return Promise.reject(new CommandOutstandingError())
    this.#sent += 1
    const id = `${this.seat}-c${String(this.#sent)}`
    return new Promise((resolve, reject) => {
      this.#pending = { id, resolve, reject }
      this.#port.postMessage({ kind: "submit", submit: { request_id: id, command } })
    })
  }

  /** Ask for this seat's complete state; a query that never advances the match. */
  baseline(): void {
    if (!this.#closed) this.#port.postMessage({ kind: "baseline" })
  }

  /** Destroy the channel; later messages from an old Worker are ignored. */
  close(): void {
    if (this.#closed) return
    this.#closed = true
    this.#port.onmessage = null
    this.#port.close()
    this.#listeners.clear()
    this.#pending?.reject(new ChannelClosedError())
    this.#pending = undefined
  }

  #receive(message: SeatMessage): void {
    if (this.#closed) return
    if (message.kind === "receipt") {
      const pending = this.#pending
      if (pending?.id !== message.request_id) return
      this.#pending = undefined
      pending.resolve(message)
      return
    }
    if (message.kind === "host_error" || message.kind === "load_error") {
      // Interaction stops; an outstanding command will never get a receipt.
      this.#pending?.reject(new ChannelClosedError())
      this.#pending = undefined
    }
    for (const listener of this.#listeners) listener(message)
  }
}

export interface LocalBattle {
  channels: Partial<Record<Seat, SeatChannel>>
  /** Close every channel and stop the Worker; a rebuild starts a new battle. */
  terminate(): void
}

export interface BattleWorker {
  postMessage(message: InitMessage, transfer: Transferable[]): void
  terminate(): void
}

function defaultWorker(): BattleWorker {
  return new Worker(new URL("./worker.ts", import.meta.url), { type: "module" })
}

export function startLocalBattle(
  inputs: MatchInputs,
  options: MatchOptions,
  createWorker: () => BattleWorker = defaultWorker,
): LocalBattle {
  const worker = createWorker()
  const channels: Partial<Record<Seat, SeatChannel>> = {}
  const ports: Partial<Record<Seat, MessagePort>> = {}
  SEATS.forEach((seat, index) => {
    if (options.control[index] !== "player") return
    const { port1, port2 } = new MessageChannel()
    channels[seat] = new SeatChannel(seat, port1)
    ports[seat] = port2
  })
  worker.postMessage({ kind: "init", inputs, options, ports }, Object.values(ports))
  return {
    channels,
    terminate() {
      for (const channel of Object.values(channels)) channel.close()
      worker.terminate()
    },
  }
}

/** Hot seat: the UI follows one viewer; switching drops the old seat's state and reloads a baseline. */
export class HotSeat {
  readonly #channels: Record<Seat, SeatChannel>
  readonly #onChange: (viewer: Seat, update: Update | undefined) => void
  #viewer: Seat
  #latest: Update | undefined
  #awaitingBaseline = true

  constructor(
    channels: Record<Seat, SeatChannel>,
    viewer: Seat,
    onChange: (viewer: Seat, update: Update | undefined) => void,
  ) {
    this.#channels = channels
    this.#viewer = viewer
    this.#onChange = onChange
    for (const seat of SEATS) {
      channels[seat].listen((message) => {
        this.#receive(seat, message)
      })
    }
  }

  get viewer(): Seat {
    return this.#viewer
  }

  get latest(): Update | undefined {
    return this.#latest
  }

  switchTo(viewer: Seat): void {
    this.#viewer = viewer
    this.#latest = undefined
    this.#awaitingBaseline = true
    this.#onChange(viewer, undefined)
    this.#channels[viewer].baseline()
  }

  /** Commands go only through the current viewer's channel. */
  submit(command: Command): Promise<Receipt> {
    return this.#channels[this.#viewer].submit(command)
  }

  #receive(seat: Seat, message: SeatMessage): void {
    if (seat !== this.#viewer || message.kind !== "update") return
    // Updates queued before the switch are older than the requested baseline.
    if (this.#awaitingBaseline && !message.baseline) return
    this.#awaitingBaseline = false
    this.#latest = message
    this.#onChange(seat, message)
  }
}
