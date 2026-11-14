// Temporary local-battle messages (format 0). The generated battle contract types replace the
// seat payloads; the Worker init and port routing below stay.

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
  format: 0
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
export type SeatRequest = { kind: "submit"; submit: Submit } | { kind: "baseline" }

/** Worker → seat port. */
export type SeatMessage = Outgoing | { kind: "load_error"; code: LoadErrorCode }
