// Main-thread side of the local battle: one channel per player seat and the hot-seat view.
import type { Notice } from "../contract"
import { NoticeInbox } from "../presentation/notices"
import type {
  BattleUpdate,
  Command,
  InitMessage,
  MatchOptions,
  Receipt,
  Seat,
  SeatMessage,
  SeatRequest,
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

/** Rejects commands after the host stopped interaction (host or load error). */
export class ChannelStoppedError extends Error {
  readonly stop: StopMessage
  constructor(stop: StopMessage) {
    super(`interaction stopped: ${stop.kind}`)
    this.stop = stop
  }
}

type StopMessage = Extract<SeatMessage, { kind: "host_error" | "load_error" }>

/** One seat's ordered channel; at most one rules command is in flight, never retried. */
export class SeatChannel {
  readonly seat: Seat
  readonly #port: ChannelPort
  readonly #listeners = new Set<(message: SeatMessage) => void>()
  readonly #inbox = new NoticeInbox()
  #pending:
    { id: string; resolve: (receipt: Receipt) => void; reject: (error: Error) => void } | undefined
  #sent = 0
  #queries = 0
  #closed = false
  #stopped: StopMessage | undefined

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

  /** The host or load error that stopped interaction, if any; it never clears. */
  get stopped(): StopMessage | undefined {
    return this.#stopped
  }

  get notices(): readonly Notice[] {
    return this.#inbox.pending()
  }

  dismissNotice(id: string): void {
    this.#inbox.dismiss(id)
  }

  setAutoAnswer(enabled: boolean): void {
    if (!this.#closed && this.#stopped === undefined)
      this.#port.postMessage({ kind: "auto_answer", enabled })
  }

  /** Every non-receipt message, in delivery order. */
  listen(listener: (message: SeatMessage) => void): () => void {
    this.#listeners.add(listener)
    return () => this.#listeners.delete(listener)
  }

  submit(command: Command): Promise<Receipt> {
    if (this.#closed) return Promise.reject(new ChannelClosedError())
    if (this.#stopped !== undefined) return Promise.reject(new ChannelStoppedError(this.#stopped))
    if (this.#pending !== undefined) return Promise.reject(new CommandOutstandingError())
    this.#sent += 1
    const id = `${this.seat}-c${String(this.#sent)}`
    return new Promise((resolve, reject) => {
      this.#pending = { id, resolve, reject }
      this.#port.postMessage({
        kind: "submit",
        submit: { kind: "submit", request_id: id, command },
      })
    })
  }

  /**
   * Ask for this seat's complete state; a query that never advances the match. Returns the
   * request id its `baseline_start` will carry, or undefined when nothing was sent.
   */
  baseline(): string | undefined {
    if (this.#closed || this.#stopped !== undefined) return undefined
    this.#queries += 1
    const id = `${this.seat}-b${String(this.#queries)}`
    this.#port.postMessage({ kind: "baseline", request_id: id })
    return id
  }

  /** Destroy the channel; later messages from an old Worker are ignored. */
  close(): void {
    if (this.#closed) return
    this.#closed = true
    this.#port.onmessage = null
    this.#port.close()
    this.#listeners.clear()
    this.#inbox.clear()
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
    if (this.#stopped !== undefined) return
    if (message.kind === "update") {
      if (message.viewer !== this.seat) return
      this.#inbox.receive(message.notices)
    }
    if (message.kind === "host_error" || message.kind === "load_error") {
      // Interaction stops for good; an outstanding command will never get a receipt.
      this.#stopped = message
      this.#pending?.reject(new ChannelStoppedError(message))
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
  worker.postMessage({ kind: "init", options, ports }, Object.values(ports))
  return {
    channels,
    terminate() {
      for (const channel of Object.values(channels)) channel.close()
      worker.terminate()
    },
  }
}

/**
 * Hot seat: the UI follows one viewer. Switching drops the old seat's state and accepts updates
 * again only after the reply to this switch's own baseline request begins. Each channel keeps
 * its own notices even while it is not the viewer. The callback's notice list is the viewer's
 * unread queue, including notices delivered in earlier updates.
 */
export class HotSeat {
  readonly #channels: Record<Seat, SeatChannel>
  readonly #onChange: (
    viewer: Seat,
    update: BattleUpdate | undefined,
    notices: readonly Notice[],
  ) => void
  #viewer: Seat
  #latest: BattleUpdate | undefined
  #awaiting: string | undefined
  #stopped: StopMessage | undefined

  constructor(
    channels: Record<Seat, SeatChannel>,
    viewer: Seat,
    onChange: (viewer: Seat, update: BattleUpdate | undefined, notices: readonly Notice[]) => void,
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

  get latest(): BattleUpdate | undefined {
    return this.#latest
  }

  /** The host or load error that stopped the battle, if any. */
  get stopped(): StopMessage | undefined {
    return this.#stopped
  }

  switchTo(viewer: Seat): void {
    this.#viewer = viewer
    this.#latest = undefined
    this.#awaiting = this.#stopped === undefined ? this.#channels[viewer].baseline() : undefined
    this.#onChange(
      viewer,
      undefined,
      this.#awaiting === undefined ? this.#channels[viewer].notices : [],
    )
  }

  setAutoAnswer(enabled: boolean): void {
    this.#channels[this.#viewer].setAutoAnswer(enabled)
  }

  dismissNotice(id: string): void {
    if (this.#awaiting !== undefined) return
    const channel = this.#channels[this.#viewer]
    channel.dismissNotice(id)
    this.#onChange(this.#viewer, this.#latest, channel.notices)
  }

  /** Commands go only through the current viewer's channel, and only while a question is shown. */
  submit(command: Command): Promise<Receipt> {
    if (this.#stopped !== undefined) return Promise.reject(new ChannelStoppedError(this.#stopped))
    return this.#channels[this.#viewer].submit(command)
  }

  #receive(seat: Seat, message: SeatMessage): void {
    if (this.#stopped !== undefined) return
    if (message.kind === "host_error" || message.kind === "load_error") {
      // The host stops every seat; nothing shown may stay answerable.
      this.#stopped = message
      this.#awaiting = undefined
      this.#latest = undefined
      this.#onChange(this.#viewer, undefined, this.#channels[this.#viewer].notices)
      return
    }
    if (seat !== this.#viewer) return
    if (message.kind === "baseline_start") {
      if (message.request_id === this.#awaiting) this.#awaiting = undefined
      return
    }
    // Anything before this switch's baseline reply was queued for the old view.
    if (message.kind !== "update" || this.#awaiting !== undefined) return
    this.#latest = message
    this.#onChange(seat, message, this.#channels[seat].notices)
  }
}
