// Local-battle messages. Every host message is the generated battle contract's `HostMessage`;
// only the Worker init, the port requests and the two local wrappers below are local.
import type { BattleUpdate, Command, HostMessage, Receipt, Seat, Submit } from "../contract"

export type { BattleUpdate, Command, Receipt, Seat }
export const SEATS: readonly Seat[] = ["p1", "p2"]
type Control = "player" | "policy"

export interface Delivery {
  seat: Seat
  message: HostMessage
}

/**
 * The battle inputs document `build:engine` wrote from one verified public export: the reader's
 * snapshot and references, the programs and rules bound to those bytes, and their identity.
 */
export type MatchInputs = string

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

export type LoadErrorCode = "format" | "selection" | "identity" | "references" | "match" | "engine"

/** Main thread → Worker, once; one transferred port per player seat. */
export interface InitMessage {
  kind: "init"
  inputs: MatchInputs
  options: MatchOptions
  ports: Partial<Record<Seat, MessagePort>>
}

/** Seat port → Worker. */
export type SeatRequest =
  | { kind: "submit"; submit: Submit }
  | { kind: "baseline"; request_id: string }
  | { kind: "auto_answer"; enabled: boolean }

/**
 * Worker → seat port. `baseline_start` precedes the deliveries answering that baseline request,
 * so the receiver can tell them from updates already queued on the port.
 */
export type SeatMessage =
  | HostMessage
  | { kind: "load_error"; code: LoadErrorCode }
  | { kind: "baseline_start"; request_id: string }
