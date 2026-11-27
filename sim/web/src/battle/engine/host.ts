// Worker-side host: one engine session, one ordered queue for every command from both seats.
import type { Answer, Submit } from "../contract"
import type {
  Delivery,
  InitMessage,
  LoadErrorCode,
  Seat,
  SeatMessage,
  SeatRequest,
} from "./protocol"
import { LOAD_ERROR_CODES, SEATS } from "./protocol"

/** The wasm-bindgen `Session`; every call is synchronous. */
export interface EngineSession {
  start(): string
  submit(seat: string, message: string): string
  baseline(seat: string): string
}

/** Loads the engine and the match inputs it was built with, then starts from the setup. */
export type CreateSession = (setup: string) => Promise<EngineSession>

type Random = (bytes: Uint8Array<ArrayBuffer>) => Uint8Array<ArrayBuffer>
type ProvenAnswer = { id: string; answer: Answer }

/** Checks a contract message before it leaves the Worker; dev builds and tests pass the generated validator. */
export type ValidateContract = (message: unknown) => boolean

export interface SeatPort {
  postMessage(message: SeatMessage): void
  onmessage: ((event: MessageEvent<SeatRequest>) => void) | null
}

function isLoadCode(value: unknown): value is LoadErrorCode {
  return LOAD_ERROR_CODES.some((code) => code === value)
}

function loadCodes(error: unknown): Readonly<Record<Seat, LoadErrorCode>> {
  const message = error instanceof Error ? error.message : ""
  if (isLoadCode(message)) return { p1: message, p2: message }
  try {
    const codes: unknown = JSON.parse(message)
    if (Array.isArray(codes) && codes.length === 2) {
      const [p1, p2]: unknown[] = codes
      if (isLoadCode(p1) && isLoadCode(p2)) return { p1, p2 }
    }
  } catch {
    // Unknown exceptions have no authorized stage code.
  }
  return { p1: "engine", p2: "engine" }
}

function randomSeed(random: Random): string {
  return Array.from(random(new Uint8Array(16)), (byte) => byte.toString(16).padStart(2, "0")).join(
    "",
  )
}

export class BattleHost {
  readonly #create: CreateSession
  readonly #random: Random
  readonly #validator: ValidateContract | Promise<ValidateContract | undefined> | undefined
  #validate: ValidateContract | undefined
  readonly #ports = new Map<Seat, SeatPort>()
  #queue: Promise<void> = Promise.resolve()
  #session: EngineSession | undefined
  #stopped = false
  readonly #enabled = new Set<Seat>()
  readonly #answers = new Map<Seat, ProvenAnswer>()
  readonly #attempted = new Map<Seat, string>()
  #scheduled = false
  #limit = 0
  #remaining = 0

  constructor(
    create: CreateSession,
    random: Random = (bytes) => crypto.getRandomValues(bytes),
    validate?: ValidateContract | Promise<ValidateContract | undefined>,
  ) {
    this.#create = create
    this.#random = random
    this.#validator = validate
  }

  init({ options, ports }: InitMessage): void {
    this.#limit = options.max_submissions
    this.#remaining = this.#limit
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
        // A validator still loading delays the engine like any other load step; commands keep queueing.
        this.#validate = await this.#validator
        this.#session = await this.#create(setup)
      } catch (error) {
        this.#stopped = true
        const codes = loadCodes(error)
        for (const [seat, port] of this.#ports) {
          port.postMessage({ kind: "load_error", code: codes[seat] })
        }
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
    if (this.#stopped || this.#session === undefined) return
    switch (request.kind) {
      case "submit":
        this.#remaining = this.#limit
        this.#call((session) => session.submit(seat, JSON.stringify(request.submit)))
        break
      case "baseline":
        this.#ports
          .get(seat)
          ?.postMessage({ kind: "baseline_start", request_id: request.request_id })
        this.#call((session) => session.baseline(seat))
        break
      case "auto_answer":
        if (request.enabled) this.#enabled.add(seat)
        else this.#enabled.delete(seat)
        this.#scheduleAutomatic()
        break
    }
  }

  #call(run: (session: EngineSession) => string): Delivery[] {
    const session = this.#session
    if (this.#stopped || session === undefined) return []
    let deliveries: Delivery[]
    try {
      deliveries = JSON.parse(run(session)) as Delivery[]
    } catch {
      // A trapped WebAssembly instance cannot be trusted again.
      this.#fail()
      return []
    }
    // Every delivery is a contract message, updates included.
    const invalid = deliveries.some(({ message }) => this.#validate?.(message) === false)
    if (invalid) {
      this.#fail()
      return []
    }
    for (const { seat, message } of deliveries) {
      if (message.kind === "host_error") this.#stopped = true
      if (message.kind === "update") {
        this.#answers.delete(seat)
        if (message.input.kind === "question") {
          const { id, purpose, auto_answer: proof } = message.input.question
          if (purpose !== "main_action" && proof.kind === "unique" && proof.public) {
            this.#answers.set(seat, { id, answer: proof.answer })
          }
        }
      }
      this.#ports.get(seat)?.postMessage(message)
    }
    this.#scheduleAutomatic()
    return deliveries
  }

  #automatic(): [Seat, ProvenAnswer] | undefined {
    if (this.#stopped) return undefined
    for (const [seat, proof] of this.#answers) {
      if (this.#enabled.has(seat) && this.#attempted.get(seat) !== proof.id) {
        return [seat, proof]
      }
    }
    return undefined
  }

  #scheduleAutomatic(): void {
    if (this.#scheduled || this.#automatic() === undefined) return
    this.#scheduled = true
    // Leave a Worker task boundary between answers so a queued preference or player command can win.
    setTimeout(() => {
      this.#enqueue(() => {
        this.#scheduled = false
        this.#advanceAutomatic()
      })
    }, 0)
  }

  #advanceAutomatic(): void {
    const next = this.#automatic()
    if (next === undefined) return
    if (this.#remaining <= 0) {
      this.#fail("budget")
      return
    }
    const [seat, { id, answer }] = next
    const submit: Submit = {
      kind: "submit",
      request_id: `local-auto-${id}`,
      command: { kind: "answer", question_id: id, answer },
    }
    this.#attempted.set(seat, id)
    this.#remaining -= 1
    const deliveries = this.#call((session) => session.submit(seat, JSON.stringify(submit)))
    const receipt = deliveries.find(
      (delivery) =>
        delivery.seat === seat &&
        delivery.message.kind === "receipt" &&
        delivery.message.request_id === submit.request_id,
    )?.message
    const current = this.#answers.get(seat)
    if (receipt?.kind === "receipt" && receipt.result !== "applied" && current !== undefined) {
      this.#attempted.set(seat, current.id)
    }
  }

  #fail(reason: "projection" | "budget" = "projection"): void {
    if (this.#stopped) return
    this.#stopped = true
    this.#broadcast({ kind: "host_error", format: 1, reason })
  }

  #broadcast(message: SeatMessage): void {
    for (const port of this.#ports.values()) port.postMessage(message)
  }
}
