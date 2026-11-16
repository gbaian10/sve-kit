// What a table draws for one seat, read straight from one observation. Nothing here works out a
// rule: numbers, postures and legality are whatever the authority put in the observation.
import type {
  BattleObservation,
  CardRef,
  EvolutionState,
  ObjectHandle,
  Phase,
  PlayerView,
  Seat,
  ZoneView,
} from "../contract"
import type { Actions } from "./actions"

export interface TableCard {
  readonly handle: ObjectHandle
  /** The physical card: under an evolve card it is the follower that evolved. */
  readonly card: CardRef
  /** The face in effect: an evolved follower shows its evolve card, not the card underneath. */
  readonly face: CardRef
  /** Lying face down, even though this seat knows it (own hand, own evolve deck). */
  readonly faceDown: boolean
  readonly acted: boolean
  readonly evolution: EvolutionState
  /** Current attack and life, for a follower the authority gives them for; absent otherwise. */
  readonly stats: { readonly attack: number; readonly life: number } | null
  readonly keywords: readonly string[]
  readonly counters: readonly { readonly name: string; readonly amount: number }[]
  /** Play points the authority currently quotes for this card; null without a settled quote. */
  readonly cost: number | null
  /** What the card may attack right now; null when it may not attack. */
  readonly attack: AttackOptions | null
}

/** The recipients the authority offers one attacker, in the open question. */
interface AttackOptions {
  /** The opposing leader is among them. */
  readonly leader: boolean
  /** The opposing field cards among them. */
  readonly targets: readonly ObjectHandle[]
}

/** A zone as a size plus the members this seat can identify, in zone order. */
export interface Pile {
  readonly count: number
  readonly cards: readonly TableCard[]
}

/**
 * Where the cards of the field and the EX area are first laid: the slot of each, by its place in
 * the zone's order. Only a table that starts with cards on it has one.
 */
export type SlotArrangement = Readonly<
  Partial<Record<Seat, { readonly field?: readonly number[]; readonly ex?: readonly number[] }>>
>

type Row = readonly (ObjectHandle | null)[]

/**
 * What lies in each slot of the field and the EX area. The contract orders a zone and does not
 * place its cards in slots, so where a card lies is the screen's own memory.
 */
export type Placement = Readonly<Record<Seat, { readonly field: Row; readonly ex: Row }>>

export const UNPLACED: Placement = { p1: { field: [], ex: [] }, p2: { field: [], ex: [] } }

/** Evolve points as markers on the table: how many there were and how many are left. */
interface Points {
  readonly current: number
  readonly max: number
}

export interface SeatTable {
  readonly seat: Seat
  readonly life: number
  readonly pp: number
  readonly maxPp: number
  readonly ep: Points
  /** `opensIn` is how many of the seat's own turns remain before a super evolution may be made. */
  readonly sep: Points & { readonly opensIn: number }
  /** Null until the first player is chosen. */
  readonly first: boolean | null
  readonly active: boolean
  /** Turns this seat has begun, the current one included. */
  readonly turns: number
  /** One entry per slot; an empty slot is null. */
  readonly field: readonly (TableCard | null)[]
  readonly ex: readonly (TableCard | null)[]
  readonly hand: Pile
  readonly deck: Pile
  readonly evolveDeck: Pile
  readonly cemetery: Pile
  readonly banish: Pile
}

export interface TableModel {
  readonly viewer: Seat
  /** Null before the opening is over. */
  readonly phase: Phase | null
  readonly self: SeatTable
  readonly opponent: SeatTable
}

/** One identifiable object as the table draws it; null for a handle this seat cannot identify. */
export function tableCard(observation: BattleObservation, handle: ObjectHandle): TableCard | null {
  const view = observation.objects[handle]
  if (view === undefined) return null
  const quote = observation.costs.cards.find((entry) => entry.object === handle)
  return {
    handle,
    card: view.card,
    face: view.effective_face,
    faceDown: view.facing === "down",
    acted: view.state?.posture === "acted",
    evolution: view.state?.evolution ?? "base",
    stats: view.state?.stats ?? null,
    keywords: view.state?.keywords ?? [],
    counters: view.state?.counters ?? [],
    cost: quote?.cost.total_pp ?? null,
    attack: null,
  }
}

function cards(
  observation: BattleObservation,
  zone: ZoneView,
): (TableCard & { at: number | null })[] {
  return zone.cards.flatMap((member) => {
    // A member without an object is not identifiable after all; it stays an anonymous card.
    const card = tableCard(observation, member.object)
    return card === null ? [] : { at: member.position, ...card }
  })
}

function pile(observation: BattleObservation, zone: ZoneView): Pile {
  const known = cards(observation, zone)
  // Members the seat can place come first in their order; unplaced ones keep delivery order.
  const placed = known.filter((card) => card.at !== null).sort((a, b) => (a.at ?? 0) - (b.at ?? 0))
  const loose = known.filter((card) => card.at === null)
  return { count: zone.count, cards: [...placed, ...loose].map(({ at: _, ...card }) => card) }
}

function row(
  observation: BattleObservation,
  laid: Row,
  zone: ZoneView,
  first: readonly number[] | undefined,
): Row {
  // A member the seat cannot identify after all is drawn nowhere, so it takes no slot. Arrivals
  // are laid in the zone's order, whatever order the members were delivered in.
  const members = zone.cards
    .filter((member) => observation.objects[member.object] !== undefined)
    .toSorted((a, b) => (a.position ?? Infinity) - (b.position ?? Infinity))
  const present = new Set(members.map((member) => member.object))
  const next = Array.from(
    { length: Math.max(zone.capacity ?? 0, zone.count) },
    (_, slot): ObjectHandle | null => {
      const held = laid[slot] ?? null
      return held !== null && present.has(held) ? held : null
    },
  )
  const kept = new Set(next)
  for (const { object, position } of members) {
    if (kept.has(object)) continue
    const wanted = position === null ? undefined : first?.[position]
    // A card without a slot of its own (or whose slot is taken) takes the first free slot
    // instead of vanishing.
    const slot =
      wanted !== undefined && wanted < next.length && next[wanted] === null
        ? wanted
        : next.indexOf(null)
    if (slot === -1) next.push(object)
    else next[slot] = object
  }
  return next
}

/**
 * The placement once `observation` is on screen. A card keeps its slot for as long as it stays in
 * its zone, whatever arrives or leaves beside it; a card that arrives takes the first free slot.
 * `first` lays the cards of a table that starts with some on it.
 */
export function place(
  laid: Placement,
  observation: BattleObservation,
  first: SlotArrangement = {},
): Placement {
  const next = { ...laid }
  for (const { seat, zones } of observation.players)
    next[seat] = {
      field: row(observation, laid[seat].field, zones.field, first[seat]?.field),
      ex: row(observation, laid[seat].ex, zones.ex, first[seat]?.ex),
    }
  return next
}

function slots(
  observation: BattleObservation,
  laid: Row,
  acts: Actions | null = null,
): (TableCard | null)[] {
  return laid.map((handle) => {
    const card = handle === null ? null : tableCard(observation, handle)
    const attack = card === null ? undefined : acts?.attack.get(card.handle)
    return card === null || attack === undefined
      ? card
      : { ...card, attack: { leader: attack.leader !== null, targets: [...attack.cards.keys()] } }
  })
}

/**
 * Evolve points as the authority reports them; which seat gets how many, and from which turn, is
 * never worked out here. The contract says whether super evolution points were handed out, not how
 * many, so a seat that has spent all of its own keeps a single faded marker.
 */
function points(player: PlayerView, turnsBegun: number): Pick<SeatTable, "ep" | "sep"> {
  const from = player.sep_available_from
  return {
    ep: { current: player.ep, max: player.ep_capacity },
    sep: {
      current: player.sep,
      max: Math.max(player.sep, from === null ? 0 : 1),
      opensIn: from === null ? 0 : Math.max(0, from - turnsBegun),
    },
  }
}

function seatTable(
  observation: BattleObservation,
  player: PlayerView,
  index: number,
  laid: Placement[Seat],
  acts: Actions | null,
): SeatTable {
  const { turn } = observation
  const turns = turn.elapsed_turns[index] ?? 0
  return {
    seat: player.seat,
    life: player.life,
    pp: player.pp,
    maxPp: player.max_pp,
    ...points(player, turns),
    first: turn.first_player === null ? null : turn.first_player === player.seat,
    active: turn.active === player.seat,
    turns,
    field: slots(observation, laid.field, acts),
    ex: slots(observation, laid.ex),
    hand: pile(observation, player.zones.hand),
    deck: pile(observation, player.zones.deck),
    evolveDeck: pile(observation, player.zones.evolve_deck),
    cemetery: pile(observation, player.zones.cemetery),
    banish: pile(observation, player.zones.banish),
  }
}

/**
 * `laid` is where this screen has its field and EX cards; it is brought up to `observation` here
 * too, so a placement that has not seen the observation never loses a card. `acts` are the actions
 * of the question open to the viewer, if there is one: they say which of the viewer's cards may
 * attack, and what.
 */
export function tableModel(
  observation: BattleObservation,
  laid: Placement = UNPLACED,
  acts: Actions | null = null,
): TableModel {
  const rows = place(laid, observation)
  const [first, second] = observation.players.map((player, index) =>
    seatTable(
      observation,
      player,
      index,
      rows[player.seat],
      // A question is put to one seat about its own cards.
      player.seat === observation.viewer ? acts : null,
    ),
  ) as [SeatTable, SeatTable]
  const selfFirst = first.seat === observation.viewer
  return {
    viewer: observation.viewer,
    phase: observation.turn.phase,
    self: selfFirst ? first : second,
    opponent: selfFirst ? second : first,
  }
}
