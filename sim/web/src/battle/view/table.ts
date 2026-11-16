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
 * What the table draws about evolve points that the contract does not carry yet. PROVISIONAL: once
 * the contract's player view has these, `points` below reads them from there and this goes away.
 */
export interface ProvisionalPoints {
  readonly epMax: number
  readonly sepMax: number
  /** The seat's own turns left before super evolution opens; 0 once it is open. */
  readonly sepOpensIn: number
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

function cards(
  observation: BattleObservation,
  zone: ZoneView,
): (TableCard & { at: number | null })[] {
  return zone.cards.flatMap((member) => {
    const view = observation.objects[member.object]
    // A member without an object is not identifiable after all; it stays an anonymous card.
    if (view === undefined) return []
    const quote = observation.costs.cards.find((entry) => entry.object === member.object)
    return {
      at: member.position,
      handle: member.object,
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
 * The one place that knows where the maximum of each point and the super-evolution wait come from.
 * Nothing here works them out: they are given, like every other number on the table.
 */
function points(player: PlayerView, provisional: ProvisionalPoints): Pick<SeatTable, "ep" | "sep"> {
  return {
    ep: { current: player.ep, max: provisional.epMax },
    sep: { current: player.sep, max: provisional.sepMax, opensIn: provisional.sepOpensIn },
  }
}

function seatTable(
  observation: BattleObservation,
  player: PlayerView,
  index: number,
  provisional: ProvisionalPoints,
  arranged: SlotArrangement[Seat],
): SeatTable {
  const { turn } = observation
  return {
    seat: player.seat,
    life: player.life,
    pp: player.pp,
    maxPp: player.max_pp,
    ...points(player, provisional),
    first: turn.first_player === null ? null : turn.first_player === player.seat,
    active: turn.active === player.seat,
    turns: turn.elapsed_turns[index] ?? 0,
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
  provisional: Readonly<Record<Seat, ProvisionalPoints>>,
  arrangement: SlotArrangement = {},
): TableModel {
  const [first, second] = observation.players.map((player, index) =>
    seatTable(observation, player, index, provisional[player.seat], arrangement[player.seat]),
  ) as [SeatTable, SeatTable]
  const selfFirst = first.seat === observation.viewer
  return {
    viewer: observation.viewer,
    phase: observation.turn.phase,
    self: selfFirst ? first : second,
    opponent: selfFirst ? second : first,
  }
}
