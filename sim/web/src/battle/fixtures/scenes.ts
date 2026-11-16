// Fixture scenes: a described table per scene and what each seat is sent for it. Numbers, postures
// and keywords are arranged to exercise the screen, not to record a position a game reached.
import type { CardRef, HostMessage, Seat } from "../contract"
import type { AttackHints, SlotArrangement } from "../view/table"
import { baseline, type FieldSpec, ready, type SeatSpec, type TableSpec } from "./build"
import { type CardNo, cardRef } from "./cards"

type SceneId = "preset" | "opening" | "mid" | "full" | "stack"

const attackHint = ({ attack }: FieldSpec) =>
  attack === undefined
    ? null
    : { leader: attack.leader === true, followers: attack.followers ?? [] }

export interface Scene {
  readonly id: SceneId
  /** The contract carries no leader card (a leader is a seat's look); match setup supplies it. */
  readonly leaders: readonly [CardRef, CardRef]
  /** Which slots the field cards lie in on screen; the messages only give their order. */
  readonly slots: SlotArrangement
  /** Which field cards may attack now, by zone order: what the seat's open question would say. */
  readonly attackers: AttackHints
  /** Everything each seat receives, in order. */
  readonly script: Readonly<Record<Seat, readonly HostMessage[]>>
}

const DRAGON_LEADER: CardNo = "BP10-LD05"
const ROYAL_LEADER: CardNo = "BP05-LD02"
const WITCH_LEADER: CardNo = "BP01-LD05"

const DRAGON_EVOLVE: readonly CardNo[] = [
  "BP05-053",
  "BP05-053",
  "BP05-053",
  "BP20-065",
  "BP20-065",
  "BP15-114",
  "BP15-114",
  "BP15-067",
  "BP20-058",
  "BP05-056",
]
const WITCH_EVOLVE: readonly CardNo[] = [
  "BP01-053",
  "BP01-053",
  "BP01-055",
  "BP01-055",
  "BP01-069",
  "BP01-069",
]
const ROYAL_EVOLVE: readonly CardNo[] = [
  "BP16-032",
  "BP16-032",
  "BP16-032",
  "BP13-027",
  "BP13-027",
  "BP13-027",
  "BP13-031",
  "BP13-031",
  "BP13-031",
  "BP13-020",
]

/** The evolve deck with these cards taken out of it, one copy each. */
function without(deck: readonly CardNo[], taken: readonly CardNo[]): CardNo[] {
  const rest = [...deck]
  for (const no of taken) {
    const at = rest.indexOf(no)
    if (at !== -1) rest.splice(at, 1)
  }
  return rest
}

// When each seat may first super evolve, counted in its own turns, as the authority hands it out.
const SEP_FROM = { first: 7, second: 6 }

function scene(
  id: SceneId,
  table: TableSpec,
  leaders: readonly [CardNo, CardNo] = [DRAGON_LEADER, ROYAL_LEADER],
): Scene {
  return {
    id,
    leaders: [cardRef(leaders[0]), cardRef(leaders[1])],
    slots: {
      p1: { field: table.seats[0].field.map((card) => card.slot) },
      p2: { field: table.seats[1].field.map((card) => card.slot) },
    },
    attackers: {
      p1: table.seats[0].field.map(attackHint),
      p2: table.seats[1].field.map(attackHint),
    },
    script: {
      p1: [ready("p1"), baseline(table, "p1")],
      p2: [ready("p2"), baseline(table, "p2")],
    },
  }
}

const opening = (hand: readonly CardNo[], evolve: readonly CardNo[]): SeatSpec => ({
  life: 20,
  pp: 0,
  maxPp: 0,
  ep: 0,
  sep: 0,
  epCapacity: 0,
  sepAvailableFrom: null,
  deck: 36,
  hand,
  field: [],
  ex: [],
  cemetery: [],
  banish: [],
  evolveDeck: { faceDown: evolve, faceUp: [] },
})

export const SCENES = [
  // The table the screen's design was tuned on: the fifth turn, the viewer's own main phase.
  scene("preset", {
    turn: { active: "p1", first_player: "p2", phase: "main", elapsed_turns: [5, 5] },
    seats: [
      {
        life: 16,
        pp: 5,
        maxPp: 5,
        ep: 2,
        sep: 1,
        epCapacity: 3,
        sepAvailableFrom: SEP_FROM.second,
        deck: 23,
        hand: ["BP05-052", "BP15-057", "BP20-063", "BP05-058", "BP15-064", "BP20-067"],
        field: [
          // The opposing engaged follower is the only one that may be attacked; the second card
          // here came into play this turn with rush, so the leader is out of its reach.
          { no: "BP20-062", slot: 1, attack: { leader: true, followers: [1] } },
          { no: "BP08-063", slot: 2, acted: true },
          { no: "BP20-064", slot: 3, keywords: ["guard"], attack: { followers: [1] } },
          { no: "BP20-062", slot: 4, stats: [3, 2] },
        ],
        ex: ["BP20-T05"],
        cemetery: ["BP08-068", "SD04-002", "BP15-113", "BP05-058"],
        banish: ["BP08-068", "BP20-067", "BP15-064"],
        evolveDeck: { faceDown: DRAGON_EVOLVE, faceUp: [] },
      },
      {
        life: 18,
        pp: 1,
        maxPp: 5,
        ep: 0,
        sep: 1,
        epCapacity: 0,
        sepAvailableFrom: SEP_FROM.first,
        deck: 23,
        hand: ["BP13-026", "BP16-021", "BP13-035", "BP16-026", "BP13-028"],
        field: [
          { no: "BP13-030", slot: 0 },
          { no: "BP13-033", slot: 2, acted: true, keywords: ["assail"] },
          { no: "BP21-034", slot: 4 },
        ],
        ex: [],
        cemetery: ["SD07-012", "BP13-032", "BP13-030", "BP21-034", "BP13-033"],
        banish: ["BP16-026", "SD07-012", "BP13-026"],
        evolveDeck: { faceDown: ROYAL_EVOLVE, faceUp: [] },
      },
    ],
  }),
  // Before the first player is chosen: no turn, no phase, opening hands only.
  scene("opening", {
    turn: { active: null, first_player: null, phase: null, elapsed_turns: [0, 0] },
    seats: [
      opening(["BP20-064", "BP05-058", "BP20-062", "BP15-057"], DRAGON_EVOLVE),
      opening(["BP13-026", "BP13-030", "BP16-026", "BP16-021"], ROYAL_EVOLVE),
    ],
  }),
  // Mid game, the second player's main phase: every card state the table draws.
  scene("mid", {
    turn: { active: "p2", first_player: "p1", phase: "main", elapsed_turns: [5, 5] },
    seats: [
      {
        life: 16,
        pp: 1,
        maxPp: 5,
        ep: 0,
        sep: 1,
        epCapacity: 0,
        sepAvailableFrom: SEP_FROM.first,
        deck: 22,
        hand: ["BP05-052", "BP15-057", "BP20-063", "BP05-058", "BP15-064", "BP20-067"],
        field: [
          { no: "BP20-062", slot: 0 },
          { no: "BP08-063", slot: 1, acted: true },
          { no: "BP20-064", slot: 2, evolved: "BP20-065", acted: true, keywords: ["guard"] },
          { no: "BP20-062", slot: 3, stats: [3, 2] },
          { no: "BP20-067", slot: 4 },
        ],
        ex: ["BP20-T05"],
        cemetery: ["BP08-068", "SD04-002", "BP15-113", "BP05-058"],
        banish: ["BP08-068", "BP20-067", "BP15-064"],
        evolveDeck: {
          faceDown: without(DRAGON_EVOLVE, ["BP20-065", "BP15-114"]),
          faceUp: ["BP15-114"],
        },
      },
      {
        life: 18,
        pp: 5,
        maxPp: 5,
        ep: 2,
        sep: 1,
        epCapacity: 3,
        sepAvailableFrom: SEP_FROM.second,
        deck: 22,
        hand: ["BP13-026", "BP16-021", "BP13-035", "BP16-026", "BP13-028", "BP21-022"],
        field: [
          // The other seat's engaged ward takes every attack: nothing here reaches the leader.
          { no: "BP13-030", slot: 0, attack: { followers: [2] } },
          { no: "BP13-033", slot: 1, acted: true },
          {
            no: "BP18-023",
            slot: 2,
            stats: [5, 5],
            keywords: ["storm"],
            attack: { followers: [2] },
          },
          { no: "BP13-026", slot: 4, evolved: "BP13-027", superEvolved: true, stats: [3, 3] },
        ],
        ex: [],
        cemetery: ["SD07-012", "BP13-032", "BP13-030", "BP21-034", "BP13-033"],
        banish: ["BP16-026", "SD07-012", "BP13-026"],
        evolveDeck: {
          faceDown: without(ROYAL_EVOLVE, ["BP13-027", "BP13-031"]),
          faceUp: ["BP13-031"],
        },
      },
    ],
  }),
  // The most crowded table: five on each field, five in each EX area, nine cards in each hand.
  scene("full", {
    turn: { active: "p1", first_player: "p1", phase: "main", elapsed_turns: [9, 8] },
    seats: [
      {
        life: 9,
        pp: 3,
        maxPp: 9,
        ep: 0,
        sep: 0,
        epCapacity: 0,
        sepAvailableFrom: SEP_FROM.first,
        deck: 14,
        hand: [
          "BP05-052",
          "BP15-057",
          "BP20-063",
          "BP05-058",
          "BP15-064",
          "BP20-067",
          "BP08-068",
          "SD04-002",
          "BP15-113",
        ],
        field: [
          { no: "BP05-052", slot: 0, evolved: "BP05-053", superEvolved: true, stats: [7, 7] },
          { no: "BP20-057", slot: 1, evolved: "BP20-058", acted: true },
          { no: "BP15-057", slot: 2, keywords: ["storm"] },
          { no: "BP20-056", slot: 3, stats: [3, 1] },
          { no: "BP20-062", slot: 4, acted: true, keywords: ["guard"] },
        ],
        ex: ["BP20-T05", "BP20-T18", "SD04-T01", "BP05-058", "BP15-064"],
        cemetery: ["BP08-068", "SD04-002", "BP15-113", "BP05-058", "BP20-064", "BP08-063"],
        banish: ["BP08-068", "BP20-067", "BP15-064", "BP08-063"],
        evolveDeck: {
          faceDown: without(DRAGON_EVOLVE, ["BP05-053", "BP20-058", "BP20-065", "BP15-114"]),
          faceUp: ["BP20-065", "BP15-114"],
        },
      },
      {
        life: 12,
        pp: 0,
        maxPp: 8,
        ep: 0,
        sep: 1,
        epCapacity: 3,
        sepAvailableFrom: SEP_FROM.second,
        deck: 12,
        hand: [
          "BP13-026",
          "BP16-021",
          "BP13-035",
          "BP16-026",
          "BP13-028",
          "BP21-022",
          "BP13-032",
          "BP16-031",
          "SD07-012",
        ],
        field: [
          { no: "BP16-021", slot: 0 },
          { no: "BP13-019", slot: 1, evolved: "BP13-020", acted: true },
          { no: "BP21-022", slot: 2, acted: true },
          { no: "BP13-028", slot: 3, stats: [6, 3] },
          { no: "SD07-016", slot: 4, keywords: ["guard"] },
        ],
        ex: ["BP16-026", "BP13-035", "BP16-026", "BP13-032", "BP16-031"],
        cemetery: ["SD07-012", "BP13-032", "BP13-030", "BP21-034", "BP13-033", "BP13-030"],
        banish: ["BP16-026", "SD07-012", "BP13-026"],
        evolveDeck: {
          faceDown: without(ROYAL_EVOLVE, ["BP13-020", "BP13-027", "BP13-031", "BP16-032"]),
          faceUp: ["BP13-027", "BP13-031", "BP16-032"],
        },
      },
    ],
  }),
  // Counters and markers: a stack counter lies only on an amulet that has Stack, so this table is a
  // Witch's. The two amulets are states the game can reach. The followers are arranged to show
  // how markers share a card: several keywords stacked, standing and engaged, and a chip beside
  // a keyword (no card gives that follower a counter today).
  scene(
    "stack",
    {
      turn: { active: "p1", first_player: "p1", phase: "main", elapsed_turns: [4, 3] },
      seats: [
        {
          life: 18,
          pp: 2,
          maxPp: 4,
          ep: 0,
          sep: 1,
          epCapacity: 0,
          sepAvailableFrom: SEP_FROM.first,
          deck: 27,
          hand: ["BP01-072", "BP01-071", "BP01-054", "BP01-052"],
          field: [
            { no: "BP01-T10", slot: 0, counters: [["スタックカウンター", 3]] },
            { no: "BP01-074", slot: 1, acted: true, counters: [["スタックカウンター", 1]] },
            { no: "BP01-067", slot: 2, keywords: ["guard", "intimidate", "bane"] },
            { no: "BP01-T09", slot: 3, acted: true, keywords: ["guard", "intimidate", "bane"] },
            { no: "BP01-068", slot: 4, keywords: ["guard"], counters: [["融合カウンター", 2]] },
          ],
          ex: [{ no: "BP01-T09", stats: [4, 5] }],
          cemetery: ["BP01-071", "BP01-068"],
          banish: [],
          evolveDeck: { faceDown: WITCH_EVOLVE, faceUp: [] },
        },
        {
          life: 17,
          pp: 0,
          maxPp: 3,
          ep: 3,
          sep: 1,
          epCapacity: 3,
          sepAvailableFrom: SEP_FROM.second,
          deck: 30,
          hand: ["BP13-026", "BP16-021", "BP13-035", "BP16-026"],
          field: [
            { no: "BP16-021", slot: 1 },
            { no: "BP13-026", slot: 3, acted: true },
          ],
          ex: ["BP16-026"],
          cemetery: ["SD07-012"],
          banish: [],
          evolveDeck: { faceDown: ROYAL_EVOLVE, faceUp: [] },
        },
      ],
    },
    [WITCH_LEADER, ROYAL_LEADER],
  ),
] as const satisfies readonly Scene[]
