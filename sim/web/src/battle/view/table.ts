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
  /** Current attack and life of a field follower; absent for anything else. */
  readonly stats: { readonly attack: number; readonly life: number } | null
  readonly keywords: readonly string[]
  readonly counters: readonly { readonly name: string; readonly amount: number }[]
  /** Play points the authority currently quotes for this card; null without a settled quote. */
  readonly cost: number | null
}

/** A zone as a size plus the members this seat can identify, in zone order. */
export interface Pile {
  readonly count: number
  readonly cards: readonly TableCard[]
}

/**
 * Which slot each card of the field and the EX area lies in, by its place in the zone's order.
 * The contract orders a zone and does not place its cards in slots, so where a card lies is the
 * screen's own arrangement; a card without one lies in the slot numbered like its place.
 */
export type SlotArrangement = Readonly<
  Partial<Record<Seat, { readonly field?: readonly number[]; readonly ex?: readonly number[] }>>
>

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

function slots(
  observation: BattleObservation,
  zone: ZoneView,
  arranged: readonly number[] | undefined,
): (TableCard | null)[] {
  const known = cards(observation, zone)
  const row: (TableCard | null)[] = Array.from(
    { length: Math.max(zone.capacity ?? 0, zone.count) },
    () => null,
  )
  const unplaced: TableCard[] = []
  for (const { at, ...card } of known) {
    const slot = at === null ? undefined : (arranged?.[at] ?? at)
    if (slot !== undefined && slot < row.length && row[slot] === null) row[slot] = card
    else unplaced.push(card)
  }
  // A card without a slot of its own (or sharing one) takes the first free slot instead of vanishing.
  for (const card of unplaced) {
    const free = row.indexOf(null)
    if (free === -1) row.push(card)
    else row[free] = card
  }
  return row
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
  arranged: SlotArrangement[Seat],
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
    field: slots(observation, player.zones.field, arranged?.field),
    ex: slots(observation, player.zones.ex, arranged?.ex),
    hand: pile(observation, player.zones.hand),
    deck: pile(observation, player.zones.deck),
    evolveDeck: pile(observation, player.zones.evolve_deck),
    cemetery: pile(observation, player.zones.cemetery),
    banish: pile(observation, player.zones.banish),
  }
}

export function tableModel(
  observation: BattleObservation,
  arrangement: SlotArrangement = {},
): TableModel {
  const [first, second] = observation.players.map((player, index) =>
    seatTable(observation, player, index, arrangement[player.seat]),
  ) as [SeatTable, SeatTable]
  const selfFirst = first.seat === observation.viewer
  return {
    viewer: observation.viewer,
    phase: observation.turn.phase,
    self: selfFirst ? first : second,
    opponent: selfFirst ? second : first,
  }
}
