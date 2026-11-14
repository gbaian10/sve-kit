// What one seat's screen plays and shows, in order (battle contract §6.2).
//
// Pure bookkeeping over complete updates: the board on screen changes only when an update's
// playback settles, committed batches play before provisional ones, a scope plays at most once
// whether or not it is promoted meanwhile, and a cancelled one is dropped. Animation, sound and
// overlays read the beats; nothing here recomputes rules.
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

/** One scope's first content and how its life ended; kept so that its id is never reused. */
interface ScopeRecord {
  /** Absent for a scope that ended without this screen ever holding it. */
  batch: PresentationBatch | null
  life: "open" | "promoted" | "cancelled"
}

export interface PresentationState {
  /** The board on screen: the last update whose playback settled. Events never edit it. */
  readonly board: BattleObservation | null
  /** The question that may be answered now; absent while anything is still playing. */
  readonly question: Question | null
  /** Accepted updates that have not settled, oldest first. */
  readonly pending: readonly PendingUpdate[]
  /** Every provisional scope ever delivered, by scope id. */
  readonly scopes: ReadonlyMap<string, ScopeRecord>
  /** Committed batches already delivered; a repeated batch is never played twice. */
  readonly delivered: ReadonlySet<string>
  /**
   * Events stopped or the engine faulted: nothing plays for the rest of the match, and
   * boards still follow every update.
   */
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

/**
 * Which committed batch each provisional occurrence became. A batch refers to a scope as a
 * whole or not at all, and no occurrence is claimed twice; otherwise `null`.
 */
function links(batches: readonly EventBatch[]): Map<string, EventBatch> | null {
  const claimed = new Map<string, EventBatch>()
  for (const batch of batches) {
    const ids = batch.events.flatMap((event) => event.presentation_id ?? [])
    if (ids.length !== 0 && ids.length !== batch.events.length) return null
    for (const id of ids) {
      if (claimed.has(id)) return null
      claimed.set(id, batch)
    }
  }
  return claimed
}

/** The one committed batch that is exactly this scope: same members, bodies and subjects. */
function promotion(
  claimed: ReadonlyMap<string, EventBatch>,
  scope: PresentationBatch,
): EventBatch | null {
  const first = scope.events[0]
  const batch = first === undefined ? undefined : claimed.get(first.id)
  if (batch?.events.length !== scope.events.length) return null
  const unchanged = batch.events.every((event, index) => {
    const shown = scope.events[index]
    return (
      shown !== undefined &&
      event.presentation_id === shown.id &&
      same(event.body, shown.body) &&
      same(event.about, shown.about)
    )
  })
  return unchanged ? batch : null
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
  const claimed = links(batches)
  if (claimed === null) return CONFLICT
  // Every occurrence ever shown. An occurrence belongs to one scope, so two scopes can never
  // stand for the same committed batch.
  const occurrences = new Set<string>()
  for (const record of scopes.values())
    for (const event of record.batch?.events ?? []) occurrences.add(event.id)
  const cancelled = new Set<string>()
  const provisional: Beat[] = []

  for (const op of update.presentation) {
    const known = scopes.get(op.scope_id)
    if (op.kind === "provisional") {
      // The same scope with the same content is a repeat, not a second performance; this
      // holds after the scope ended too, because a scope id is never reused.
      if (known !== undefined) {
        if (known.batch === null || !same(known.batch, op.batch)) return CONFLICT
        continue
      }
      if (op.batch.events.length === 0) return CONFLICT
      for (const event of op.batch.events) {
        if (occurrences.has(event.id)) return CONFLICT
        occurrences.add(event.id)
      }
      scopes.set(op.scope_id, { batch: op.batch, life: "open" })
      provisional.push({
        kind: "provisional",
        update_seq: update.update_seq,
        scope_id: op.scope_id,
        batch: op.batch,
      })
      continue
    }
    // A scope ends once. One this screen never held (a fresh seat baseline) only becomes
    // history, but its id is recorded as used.
    if (known !== undefined && known.life !== "open") return CONFLICT
    if (op.kind === "promote") {
      if (known !== undefined && known.batch !== null && promotion(claimed, known.batch) === null)
        return CONFLICT
      scopes.set(op.scope_id, { batch: known?.batch ?? null, life: "promoted" })
    } else {
      cancelled.add(op.scope_id)
      scopes.set(op.scope_id, { batch: known?.batch ?? null, life: "cancelled" })
    }
  }

  // A committed batch that refers to a scope still open must promote it in this same update.
  for (const record of scopes.values()) {
    if (record.life !== "open" || record.batch === null) continue
    if (record.batch.events.some((event) => claimed.has(event.id))) return CONFLICT
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

  // Only a cancellation removes a queued scope. A promoted one still plays its own queued
  // beat in order: promotion just keeps the committed batch from playing as well.
  const withdrawn = (beat: Beat) => beat.kind === "provisional" && cancelled.has(beat.scope_id)
  // After a fault or stopped events nothing is worth waiting for: only boards remain to align.
  const silent =
    state.silent || update.events.status === "unavailable" || update.input.kind === "engine_fault"
  const pending = state.pending.map((entry) => ({
    update: entry.update,
    beats: silent ? [] : entry.beats.filter((beat) => !withdrawn(beat)),
  }))
  pending.push({
    update,
    beats: silent ? [] : [...committed, ...provisional.filter((beat) => !withdrawn(beat))],
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
