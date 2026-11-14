// Worker-side host: one engine session, one ordered queue for every command from both seats.
import type {
  Delivery,
  InitMessage,
  LoadErrorCode,
  MatchInputs,
  Seat,
  SeatMessage,
  SeatRequest,
} from "./protocol"
import { SEATS } from "./protocol"

/** The wasm-bindgen `Session`; every call is synchronous. */
export interface EngineSession {
  start(): string
  submit(seat: string, message: string): string
  baseline(seat: string): string
}

export type CreateSession = (inputs: MatchInputs, setup: string) => Promise<EngineSession>

type Random = (bytes: Uint8Array<ArrayBuffer>) => Uint8Array<ArrayBuffer>

/** Checks a contract message before it leaves the Worker; dev builds and tests pass the generated validator. */
export type ValidateContract = (message: unknown) => boolean

export interface SeatPort {
  postMessage(message: SeatMessage): void
  onmessage: ((event: MessageEvent<SeatRequest>) => void) | null
}

const LOAD_CODES: readonly string[] = ["format", "selection", "match"]

function loadCode(error: unknown): LoadErrorCode {
  const message = error instanceof Error ? error.message : ""
  return LOAD_CODES.includes(message) ? (message as LoadErrorCode) : "engine"
}

function randomSeed(random: Random): string {
  return Array.from(random(new Uint8Array(16)), (byte) => byte.toString(16).padStart(2, "0")).join(
    "",
  )
}

export class BattleHost {
  readonly #create: CreateSession
  readonly #random: Random
  readonly #validate: ValidateContract | undefined
  readonly #ports = new Map<Seat, SeatPort>()
  #queue: Promise<void> = Promise.resolve()
  #session: EngineSession | undefined
  #stopped = false

  constructor(
    create: CreateSession,
    random: Random = (bytes) => crypto.getRandomValues(bytes),
    validate?: ValidateContract,
  ) {
    this.#create = create
    this.#random = random
    this.#validate = validate
  }

  init({ inputs, options, ports }: InitMessage): void {
    for (const seat of SEATS) {
      const port: SeatPort | undefined = ports[seat]
      if (port === undefined) continue
      this.#ports.set(seat, port)
      // Requests that arrive while the engine loads wait in the same queue.
      port.onmessage = (event) => {
        this.#enqueue(() => {
          this.#handle(seat, event.data)
        })
      }
    }
    const first = this.#random(new Uint8Array(1))[0] ?? 0
    const setup = JSON.stringify({
      decks: options.decks,
      first_chooser: options.first_chooser ?? (first % 2 === 0 ? "p1" : "p2"),
      seed: options.seed ?? randomSeed(this.#random),
      control: options.control,
      max_submissions: options.max_submissions,
    })
    this.#enqueue(async () => {
      try {
        this.#session = await this.#create(inputs, setup)
      } catch (error) {
        this.#stopped = true
        this.#broadcast({ kind: "load_error", code: loadCode(error) })
        return
      }
      this.#call((session) => session.start())
    })
  }

  #enqueue(task: () => void | Promise<void>): void {
    // One task at a time, in arrival order; an unexpected failure stops the host, not the queue.
    this.#queue = this.#queue.then(task).catch(() => {
      this.#fail()
    })
  }

  #handle(seat: Seat, request: SeatRequest): void {
    if (request.kind === "submit") {
      this.#call((session) => session.submit(seat, JSON.stringify(request.submit)))
    } else if (!this.#stopped && this.#session !== undefined) {
      this.#ports.get(seat)?.postMessage({ kind: "baseline_start", request_id: request.request_id })
      this.#call((session) => session.baseline(seat))
    }
  }

  #call(run: (session: EngineSession) => string): void {
    const session = this.#session
    if (this.#stopped || session === undefined) return
    let deliveries: Delivery[]
    try {
      deliveries = JSON.parse(run(session)) as Delivery[]
    } catch {
      // A trapped WebAssembly instance cannot be trusted again.
      this.#fail()
      return
    }
    // The temporary update has no contract schema yet; receipts and host errors are contract messages.
    const invalid = deliveries.some(
      ({ message }) => message.kind !== "update" && this.#validate?.(message) === false,
    )
    if (invalid) {
      this.#fail()
      return
    }
    for (const { seat, message } of deliveries) this.#ports.get(seat)?.postMessage(message)
  }

  #fail(): void {
    if (this.#stopped) return
    this.#stopped = true
    this.#broadcast({ kind: "host_error", format: 1, reason: "projection" })
  }

  #broadcast(message: SeatMessage): void {
    for (const port of this.#ports.values()) port.postMessage(message)
  }
}
