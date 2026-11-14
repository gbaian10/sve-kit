// Local-battle messages. Receipt and HostError follow the battle contract (format 1); Update and
// Submit stay temporary (format 0) until the generated contract types replace these declarations.
// The Worker init and port routing below stay.

export type Seat = "p1" | "p2"
export const SEATS: readonly Seat[] = ["p1", "p2"]
type Control = "player" | "policy"

type InputState =
  | { kind: "question"; id: string; purpose: string; shape: string }
  | { kind: "waiting"; actor: Seat }
  | { kind: "finished"; winner: Seat | null }
  | { kind: "engine_fault"; can_concede: boolean }

export interface Update {
  kind: "update"
  format: 0
  viewer: Seat
  update_seq: number
  baseline: boolean
  input: InputState
}

export interface Receipt {
  kind: "receipt"
  request_id: string
  result: "applied" | "rejected" | "reverted" | "cancelled" | "engine_fault"
  rejection: "invalid_submission" | null
}

interface HostError {
  kind: "host_error"
  format: 1
  reason: "format" | "range" | "projection" | "budget"
}

type Outgoing = Update | Receipt | HostError

export interface Delivery {
  seat: Seat
  message: Outgoing
}

export type Command =
  { kind: "answer"; question_id: string; answer: { kind: "policy" } } | { kind: "concede" }

interface Submit {
  request_id: string
  command: Command
}

/** Engine inputs exactly as downloaded. */
export interface MatchInputs {
  snapshot: string
  keywords: string
  selection: string
  rules: string
}

interface DeckSetup {
  construction: { kind: "class" | "title"; value: string }
  main: string[]
  evolve: string[]
}

/** The seed and first chooser are drawn inside the Worker unless a test fixes them. */
export interface MatchOptions {
  decks: [DeckSetup, DeckSetup]
  control: [Control, Control]
  max_submissions: number
  seed?: string
  first_chooser?: Seat
}

export type LoadErrorCode = "format" | "selection" | "match" | "engine"

/** Main thread → Worker, once; one transferred port per player seat. */
export interface InitMessage {
  kind: "init"
  inputs: MatchInputs
  options: MatchOptions
  ports: Partial<Record<Seat, MessagePort>>
}

/** Seat port → Worker. */
export type SeatRequest =
  { kind: "submit"; submit: Submit } | { kind: "baseline"; request_id: string }

/**
 * Worker → seat port. `baseline_start` precedes the deliveries answering that baseline request,
 * so the receiver can tell them from updates already queued on the port.
 */
export type SeatMessage =
  | Outgoing
  | { kind: "load_error"; code: LoadErrorCode }
  | { kind: "baseline_start"; request_id: string }
