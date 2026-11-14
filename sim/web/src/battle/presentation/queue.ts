// What one seat's screen plays and shows, in order (battle contract §6.2).
//
// Pure bookkeeping over complete updates: the board on screen changes only when an update's
// playback settles, committed batches play before provisional ones, a promoted scope is never
// played again and a cancelled one is dropped. Animation, sound and overlays read the beats;
// nothing here recomputes rules.
import type {
  BattleObservation,
  BattleUpdate,
  EventBatch,
  PresentationBatch,
  Question,
} from "../contract"

/** One complete batch played as a unit; members of a beat are simultaneous. */
export type Beat =
  | { kind: "committed"; update_seq: number; batch: EventBatch }
  | { kind: "provisional"; update_seq: number; scope_id: string; batch: PresentationBatch }

interface PendingUpdate {
  update: BattleUpdate
  beats: readonly Beat[]
}

export interface PresentationState {
  /** The board on screen: the last update whose playback settled. Events never edit it. */
  readonly board: BattleObservation | null
  /** The question that may be answered now; absent while anything is still playing. */
  readonly question: Question | null
  /** Accepted updates that have not settled, oldest first. */
  readonly pending: readonly PendingUpdate[]
  /** Every provisional scope still open, by scope id, as first delivered. */
  readonly scopes: ReadonlyMap<string, PresentationBatch>
  /** Committed batches already delivered; a repeated batch is never played twice. */
  readonly delivered: ReadonlySet<string>
  /** Event and presentation playback stopped; boards still follow every update. */
  readonly silent: boolean
}

/** The update contradicts something already delivered; interaction must stop. */
export interface PresentationError {
  kind: "host_error"
  reason: "projection"
}

export const EMPTY: PresentationState = {
  board: null,
  question: null,
  pending: [],
  scopes: new Map(),
  delivered: new Set(),
  silent: false,
}

const CONFLICT: PresentationError = { kind: "host_error", reason: "projection" }

/** Plain-data equality; messages are JSON or an equivalent structured clone. */
function same(left: unknown, right: unknown): boolean {
  if (left === right) return true
  if (typeof left !== "object" || typeof right !== "object" || left === null || right === null)
    return false
  if (Array.isArray(left) !== Array.isArray(right)) return false
  const keys = Object.keys(left)
  if (keys.length !== Object.keys(right).length) return false
  return keys.every(
    (key) =>
      Object.hasOwn(right, key) &&
      same((left as Record<string, unknown>)[key], (right as Record<string, unknown>)[key]),
  )
}

/** Whether the committed batches of this update are exactly the scope's occurrences. */
function promotes(batches: readonly EventBatch[], scope: PresentationBatch): boolean {
  const linked = batches.filter((batch) =>
    batch.events.some((event) => event.presentation_id !== null),
  )
  const ids = scope.events.map((event) => event.id)
  return linked.some(
    (batch) =>
      batch.events.length === ids.length &&
      batch.events.every(
        (event, index) =>
          event.presentation_id === ids[index] && same(event.body, scope.events[index]?.body),
      ),
  )
}

/**
 * Register one complete update: promotions, cancellations and duplicates first, then its
 * committed batches to animate, then its first-time provisional batches.
 */
export function accept(
  state: PresentationState,
  update: BattleUpdate,
): PresentationState | PresentationError {
  const scopes = new Map(state.scopes)
  const delivered = new Set(state.delivered)
  const batches = update.events.status === "available" ? update.events.batches : []
  const promoted = new Set<string>()
  const cancelled = new Set<string>()
  const provisional: Beat[] = []

  for (const op of update.presentation) {
    const known = scopes.get(op.scope_id)
    if (op.kind === "provisional") {
      // The same scope with the same content is a repeat, not a second performance.
      if (known !== undefined) {
        if (!same(known, op.batch)) return CONFLICT
        continue
      }
      if (op.batch.events.length === 0) return CONFLICT
      scopes.set(op.scope_id, op.batch)
      provisional.push({
        kind: "provisional",
        update_seq: update.update_seq,
        scope_id: op.scope_id,
        batch: op.batch,
      })
    } else if (op.kind === "promote") {
      // A scope this screen never held (a fresh seat baseline) only becomes history.
      if (known !== undefined && !promotes(batches, known)) return CONFLICT
      promoted.add(op.scope_id)
      scopes.delete(op.scope_id)
    } else {
      cancelled.add(op.scope_id)
      scopes.delete(op.scope_id)
    }
  }

  const animate = update.events.status === "available" ? update.events.animate : []
  if (animate.some((id) => !batches.some((batch) => batch.id === id))) return CONFLICT
  const committed: Beat[] = []
  for (const batch of batches) {
    const repeated = delivered.has(batch.id)
    delivered.add(batch.id)
    // A batch shown early is history now; playing it would perform the same result twice.
    const shownEarly = batch.events.some((event) => event.presentation_id !== null)
    if (!repeated && !shownEarly && animate.includes(batch.id))
      committed.push({ kind: "committed", update_seq: update.update_seq, batch })
  }

  // Scopes that ended in this update leave every queue; one already played stays played.
  const ended = (beat: Beat) =>
    beat.kind === "provisional" && (promoted.has(beat.scope_id) || cancelled.has(beat.scope_id))
  const silent = state.silent || update.events.status === "unavailable"
  // A fault or stopped events leave nothing to wait for: only boards remain to align.
  const quiet = silent || update.input.kind === "engine_fault"
  const pending = state.pending.map((entry) => ({
    update: entry.update,
    beats: quiet ? [] : entry.beats.filter((beat) => !ended(beat)),
  }))
  pending.push({
    update,
    beats: quiet ? [] : [...committed, ...provisional.filter((beat) => !ended(beat))],
  })
  return { ...state, question: null, pending, scopes, delivered, silent }
}

/**
 * The next beat to play, or `null` once every accepted update has settled. An update whose
 * beats are finished settles on the way: its complete board replaces the one on screen.
 */
export function advance(state: PresentationState): {
  state: PresentationState
  beat: Beat | null
} {
  let { board, question } = state
  const pending = [...state.pending]
  for (;;) {
    const current = pending[0]
    if (current === undefined) return { state: { ...state, board, question, pending }, beat: null }
    const [beat, ...rest] = current.beats
    if (beat !== undefined) {
      pending[0] = { update: current.update, beats: rest }
      return { state: { ...state, board, question: null, pending }, beat }
    }
    pending.shift()
    board = current.update.observation
    // Only the newest update's question is still valid: a later update settling in this same
    // pass replaces it, and a beat handed out before that hides it.
    question = current.update.input.kind === "question" ? current.update.input.question : null
  }
}

/** Drop everything still queued and align with the newest update at once. */
export function skip(state: PresentationState): PresentationState {
  return advance({
    ...state,
    pending: state.pending.map((entry) => ({ update: entry.update, beats: [] })),
  }).state
}
