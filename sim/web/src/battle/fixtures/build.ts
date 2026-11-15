// Turns one described table into the complete update each seat would receive. The description
// holds everything (both hands, both evolve decks); a seat's observation keeps only what that
// seat may know, so two screens fed from one scene differ the way two real seats do.
import type {
  BattleObservation,
  BattleUpdate,
  CardCostQuote,
  CardState,
  ObjectHandle,
  ObjectView,
  PlayerView,
  Ready,
  Relationship,
  Seat,
  TurnView,
  ZoneMember,
  ZoneView,
} from "../contract"
import { type CardNo, cardRef, cardTypeOf, fixtureCard } from "./cards"

interface FieldSpec {
  readonly no: CardNo
  readonly slot: number
  readonly acted?: boolean
  /** The evolve card lying on top. */
  readonly evolved?: CardNo
  readonly superEvolved?: boolean
  /** Current numbers, when they are not the ones printed on the face in effect. */
  readonly stats?: readonly [attack: number, life: number]
  readonly keywords?: readonly string[]
  readonly counters?: readonly (readonly [name: string, amount: number])[]
}

export interface SeatSpec {
  readonly life: number
  readonly pp: number
  readonly maxPp: number
  readonly ep: number
  readonly sep: number
  /** Not in the contract yet: see `ProvisionalPoints`. */
  readonly epMax: number
  readonly sepMax: number
  readonly sepOpensIn: number
  /** Nobody knows the deck's cards; only its size is part of the table. */
  readonly deck: number
  readonly hand: readonly CardNo[]
  readonly field: readonly FieldSpec[]
  readonly ex: readonly CardNo[]
  readonly cemetery: readonly CardNo[]
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
  let issued = 0

  const add = (
    owner: Seat,
    no: CardNo,
    known: "public" | "private",
    state: CardState | null = null,
    effective: CardNo = no,
  ): ObjectHandle => {
    issued += 1
    const handle = `${prefix}${String(issued)}`
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

  const listed = (owner: Seat, cards: readonly CardNo[], capacity: number | null): ZoneView => ({
    count: cards.length,
    capacity,
    cards: cards.map((no, position) => ({ position, object: add(owner, no, "public") })),
  })

  const player = (seat: Seat, spec: SeatSpec): PlayerView => {
    const own = seat === viewer
    const hand: ZoneMember[] = own
      ? spec.hand.map((no, position) => {
          const object = add(seat, no, "private")
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
    const field: ZoneMember[] = spec.field.map((card) => {
      const object = add(seat, card.no, "public", fieldState(card), card.evolved ?? card.no)
      if (card.evolved !== undefined) {
        const top = add(seat, card.evolved, "public")
        evolution.push({ position: null, object: top })
        relationships.push({ kind: "evolution", from: top, to: object })
      }
      return { position: card.slot, object }
    })
    const evolveDeck: ZoneMember[] = [
      ...spec.evolveDeck.faceUp.map((no) => ({ position: null, object: add(seat, no, "public") })),
      ...(own
        ? spec.evolveDeck.faceDown.map((no) => ({
            position: null,
            object: add(seat, no, "private"),
          }))
        : []),
    ]
    return {
      seat,
      life: spec.life,
      pp: spec.pp,
      max_pp: spec.maxPp,
      ep: spec.ep,
      sep: spec.sep,
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
        ex: listed(seat, spec.ex, ZONE_CAPACITY),
        cemetery: listed(seat, spec.cemetery, null),
        banish: listed(seat, spec.banish, null),
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

/** The first update of a seat: the whole table, nothing to play, the seat only looks on. */
export function baseline(table: TableSpec, viewer: Seat): BattleUpdate {
  return {
    kind: "update",
    format: 1,
    viewer,
    update_seq: 0,
    observation: observe(table, viewer),
    input: {
      kind: "waiting",
      actor: table.turn.active ?? "p1",
      context: { processing: null, dice: null },
    },
    events: { status: "available", batches: [], animate: [] },
    presentation: [],
    notices: [],
    advice: [],
  }
}
