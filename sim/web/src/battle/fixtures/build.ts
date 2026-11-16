// Turns one described table into the complete update each seat would receive. The description
// holds everything (both hands, both evolve decks); a seat's observation keeps only what that
// seat may know, so two screens fed from one scene differ the way two real seats do.
import type {
  BattleInput,
  BattleObservation,
  BattleUpdate,
  CardCostQuote,
  CardState,
  MainIntent,
  ObjectHandle,
  ObjectView,
  PlayerView,
  Question,
  QuestionId,
  Ready,
  Relationship,
  Seat,
  TurnView,
  ZoneMember,
  ZoneView,
} from "../contract"
import { type CardNo, cardRef, cardTypeOf, fixtureCard } from "./cards"

/**
 * A card that keeps its handle wherever it moves between the updates of a scene: a real host names
 * an object once. A card without a key is named by the zone it lies in, so it keeps its handle for
 * as long as it stays there.
 */
interface Keyed {
  readonly no: CardNo
  readonly key: string
}
type Held = CardNo | Keyed

const noOf = (card: Held): CardNo => (typeof card === "string" ? card : card.no)
const keyOf = (card: Held): string | undefined => (typeof card === "string" ? undefined : card.key)

export interface FieldSpec {
  readonly no: CardNo
  /** See `Keyed`. */
  readonly key?: string
  /**
   * Where the screen lays it in the table a scene starts with; afterwards the screen keeps its own
   * placement. Not part of what a seat is told: the contract only orders the zone.
   */
  readonly slot: number
  readonly acted?: boolean
  /** The evolve card lying on top. */
  readonly evolved?: CardNo
  readonly superEvolved?: boolean
  /** Current numbers, when they are not the ones printed on the face in effect. */
  readonly stats?: readonly [attack: number, life: number]
  readonly keywords?: readonly string[]
  /**
   * What the card may attack right now: the opposing leader, and opposing field cards by their
   * place in that seat's list. Not part of the observation: the authority says it in the question
   * it puts to the seat.
   */
  readonly attack?: { readonly leader?: true; readonly followers?: readonly number[] }
  /** Counters by the rules name the authority reports, such as スタックカウンター. */
  readonly counters?: readonly (readonly [name: string, amount: number])[]
}

export interface SeatSpec {
  readonly life: number
  readonly pp: number
  readonly maxPp: number
  readonly ep: number
  readonly sep: number
  /** Evolve point slots: the ones left and the ones spent. */
  readonly epCapacity: number
  /** The seat's own turn from which it may super evolve; null before the points are handed out. */
  readonly sepAvailableFrom: number | null
  /** Nobody knows the deck's cards; only its size is part of the table. */
  readonly deck: number
  readonly hand: readonly Held[]
  /**
   * Which hand cards the seat may play now, by their place in the hand. Like `FieldSpec.attack` it
   * is no part of the observation: the authority says it in the question it puts to the seat.
   */
  readonly playable?: readonly number[]
  readonly field: readonly FieldSpec[]
  /**
   * A card with numbers is a follower whose current attack and life the seat is told: the
   * authority reports them for followers in the EX area as it does on the field.
   */
  readonly ex: readonly (
    CardNo | { readonly no: CardNo; readonly stats: readonly [number, number] }
  )[]
  readonly cemetery: readonly Held[]
  readonly banish: readonly CardNo[]
  readonly evolveDeck: {
    /** Known to their owner only. */
    readonly faceDown: readonly CardNo[]
    /** Used evolve cards, public. */
    readonly faceUp: readonly CardNo[]
  }
}

export interface TableSpec {
  readonly turn: TurnView
  readonly seats: readonly [SeatSpec, SeatSpec]
}

const SEATS: readonly [Seat, Seat] = ["p1", "p2"]
const ZONE_CAPACITY = 5

const EMPTY: ZoneView = { count: 0, capacity: null, cards: [] }

function fieldState(spec: FieldSpec): CardState {
  const stats = spec.stats ?? fixtureCard(spec.evolved ?? spec.no).stats
  return {
    posture: spec.acted === true ? "acted" : "standing",
    evolution:
      spec.evolved === undefined
        ? "base"
        : spec.superEvolved === true
          ? "super_evolved"
          : "evolved",
    stats: stats === null ? null : { attack: stats[0], life: stats[1] },
    keywords: [...(spec.keywords ?? [])],
    counters: (spec.counters ?? []).map(([name, amount]) => ({ name, amount })),
    boxed: false,
    effects: [],
    quotas: null,
  }
}

/** One seat's complete view of the table. */
function observe(table: TableSpec, viewer: Seat): BattleObservation {
  const objects: Record<ObjectHandle, ObjectView> = {}
  const relationships: Relationship[] = []
  const quotes: CardCostQuote[] = []
  // Handles belong to one viewer: the two seats never share a name for the same card.
  const prefix = viewer === "p1" ? "a" : "b"
  const copies = new Map<string, number>()

  const add = (
    owner: Seat,
    zone: string,
    no: CardNo,
    known: "public" | "private",
    state: CardState | null = null,
    effective: CardNo = no,
    key?: string,
  ): ObjectHandle => {
    // Named by owner, zone and card rather than by a running number: what arrives or leaves
    // elsewhere on the table then renames nothing, as with the objects of a real game.
    const place = `${owner}.${zone}.${no}`
    const copy = (copies.get(place) ?? 0) + 1
    if (key === undefined) copies.set(place, copy)
    const name = key ?? (copy === 1 ? place : `${place}#${String(copy)}`)
    const handle = `${prefix}:${name}`
    objects[handle] = {
      card: cardRef(no),
      effective_face: cardRef(effective),
      owner,
      controller: owner,
      knowledge: known,
      facing: known === "public" ? "up" : "down",
      characteristics: {
        rules_names: [fixtureCard(effective).name],
        card_type: cardTypeOf(effective),
        traits: [],
        classes: null,
        cost: null,
      },
      state,
    }
    return handle
  }

  const listed = (owner: Seat, zone: string, cards: readonly Held[]): ZoneView => ({
    count: cards.length,
    capacity: null,
    cards: cards.map((card, position) => ({
      position,
      object: add(owner, zone, noOf(card), "public", null, noOf(card), keyOf(card)),
    })),
  })

  const player = (seat: Seat, spec: SeatSpec): PlayerView => {
    const own = seat === viewer
    const hand: ZoneMember[] = own
      ? spec.hand.map((card, position) => {
          const no = noOf(card)
          const object = add(seat, "hand", no, "private", null, no, keyOf(card))
          const cost = fixtureCard(no).cost
          if (cost !== null)
            quotes.push({
              object,
              cost: {
                basis: "current_context",
                total_pp: cost,
                groups: [
                  {
                    role: { kind: "original" },
                    lines: [{ kind: "pp", quote: { kind: "known", amount: cost } }],
                  },
                ],
                pending: [],
              },
            })
          return { position, object }
        })
      : []
    const evolution: ZoneMember[] = []
    const field: ZoneMember[] = spec.field.map((card, position) => {
      const object = add(
        seat,
        "field",
        card.no,
        "public",
        fieldState(card),
        card.evolved ?? card.no,
        card.key,
      )
      if (card.evolved !== undefined) {
        const top = add(seat, "evolution", card.evolved, "public")
        evolution.push({ position: null, object: top })
        relationships.push({ kind: "evolution", from: top, to: object })
      }
      return { position, object }
    })
    const evolveDeck: ZoneMember[] = [
      ...spec.evolveDeck.faceUp.map((no) => ({
        position: null,
        object: add(seat, "evolve", no, "public"),
      })),
      ...(own
        ? spec.evolveDeck.faceDown.map((no) => ({
            position: null,
            object: add(seat, "evolve", no, "private"),
          }))
        : []),
    ]
    return {
      seat,
      life: spec.life,
      pp: spec.pp,
      max_pp: spec.maxPp,
      ep: spec.ep,
      ep_capacity: spec.epCapacity,
      ep_used: spec.epCapacity - spec.ep,
      sep: spec.sep,
      sep_available_from: spec.sepAvailableFrom,
      effects: [],
      zones: {
        deck: { count: spec.deck, capacity: null, cards: [] },
        evolve_deck: {
          count: spec.evolveDeck.faceDown.length + spec.evolveDeck.faceUp.length,
          capacity: null,
          cards: evolveDeck,
        },
        hand: { count: spec.hand.length, capacity: null, cards: hand },
        field: { count: field.length, capacity: ZONE_CAPACITY, cards: field },
        ex: {
          count: spec.ex.length,
          capacity: ZONE_CAPACITY,
          cards: spec.ex.map((card, position) => ({
            position,
            object:
              typeof card === "string"
                ? add(seat, "ex", card, "public")
                : add(
                    seat,
                    "ex",
                    card.no,
                    "public",
                    fieldState({ no: card.no, slot: 0, stats: card.stats }),
                  ),
          })),
        },
        cemetery: listed(seat, "cemetery", spec.cemetery),
        banish: listed(seat, "banish", spec.banish),
        resolution: EMPTY,
        evolution: { count: evolution.length, capacity: null, cards: evolution },
        race: EMPTY,
        drive: EMPTY,
        trigger: EMPTY,
        equipment: EMPTY,
      },
    }
  }

  const players: [PlayerView, PlayerView] = [
    player(SEATS[0], table.seats[0]),
    player(SEATS[1], table.seats[1]),
  ]
  return {
    viewer,
    objects,
    turn: table.turn,
    attack: null,
    players,
    relationships,
    processing: [],
    costs: { cards: quotes, current_play: null },
    pending: [],
    history: {
      subjects: [],
      damage: [],
      stat_gains: [],
      dice: [],
      played: [0, 0],
      union_bursts: [0, 0],
    },
    remembered: [],
  }
}

const digest = (last: number): string => String(last).padStart(64, "0")

export function ready(viewer: Seat): Ready {
  return {
    kind: "ready",
    format: 1,
    viewer,
    identity: {
      snapshot: { id: "snapshot-fixture", sha256: digest(1) },
      programs: { id: "programs-fixture", sha256: digest(2) },
      engine: { id: "engine-fixture", sha256: digest(3) },
      rules: { id: "rules-fixture", sha256: digest(4) },
      region: "jp",
    },
  }
}

/**
 * The main-phase question the authority would put to `viewer`: the plays, attacks and the end of
 * the phase that the table's description lists. The description says what is legal; nothing here
 * works it out.
 */
function mainQuestion(
  observation: BattleObservation,
  table: TableSpec,
  viewer: Seat,
  id: QuestionId,
): Question {
  const [first, second] = observation.players
  const [spec, own, other] =
    viewer === first.seat ? [table.seats[0], first, second] : [table.seats[1], second, first]
  const handle = (zone: ZoneView, place: number): ObjectHandle => {
    const member = zone.cards[place]
    if (member === undefined) throw new Error(`no card at place ${String(place)}`)
    return member.object
  }
  const intents: MainIntent[] = [
    ...(spec.playable ?? []).map((place): MainIntent => {
      const source = handle(own.zones.hand, place)
      return {
        kind: "play",
        id: `play:${source}`,
        source,
        cost: observation.costs.cards.find((quote) => quote.object === source)?.cost ?? null,
      }
    }),
    ...spec.field.flatMap((card, place): MainIntent[] => {
      if (card.attack === undefined) return []
      const attacker = handle(own.zones.field, place)
      return [
        {
          kind: "attack",
          id: `attack:${attacker}`,
          attacker,
          targets: [
            ...(card.attack.followers ?? []).map((target) => {
              const object = handle(other.zones.field, target)
              return {
                id: `attack:${attacker}>${object}`,
                target: { kind: "card" as const, object },
              }
            }),
            ...(card.attack.leader === true
              ? [
                  {
                    id: `attack:${attacker}>leader`,
                    target: { kind: "leader" as const, player: other.seat },
                  },
                ]
              : []),
          ],
        },
      ]
    }),
    { kind: "end_phase", id: "end" },
  ]
  return {
    id,
    actor: viewer,
    purpose: "main_action",
    use: "main_action",
    processing: {
      stage: "declare",
      controller: viewer,
      source: null,
      subject: null,
      rule: null,
      procedure: null,
      declared: [],
      payment: null,
    },
    shape: { kind: "action_window", intents },
    rules: null,
    auto_answer: { kind: "not_unique" },
    assistance: null,
  }
}

/**
 * A complete update for one seat: the whole table, nothing to play back, and what happens next.
 * With a question id the seat whose main phase it is gets the main-phase question and the other
 * seat waits for it; `next` can also be any other input, the same for both seats.
 */
export function update(
  table: TableSpec,
  viewer: Seat,
  seq: number,
  next: QuestionId | BattleInput | null = null,
): BattleUpdate {
  const observation = observe(table, viewer)
  const active = table.turn.active ?? "p1"
  const input: BattleInput =
    typeof next === "string" && viewer === active && table.turn.phase === "main"
      ? { kind: "question", question: mainQuestion(observation, table, viewer, next) }
      : next !== null && typeof next !== "string"
        ? next
        : { kind: "waiting", actor: active, context: { processing: null, dice: null } }
  return {
    kind: "update",
    format: 1,
    viewer,
    update_seq: seq,
    observation,
    input,
    events: { status: "available", batches: [], animate: [] },
    presentation: [],
    notices: [],
    advice: [],
  }
}
