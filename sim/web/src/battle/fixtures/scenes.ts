// Fixture scenes: a described table per scene and what each seat is sent for it. Numbers, postures
// and keywords are arranged to exercise the screen, not to record a position a game reached.
import type { CardRef, HostMessage, Seat } from "../contract"
import type { ProvisionalPoints } from "../view/table"
import { baseline, ready, type SeatSpec, type TableSpec } from "./build"
import { type CardNo, cardRef } from "./cards"

type SceneId = "preset" | "opening" | "mid" | "full"

export interface Scene {
  readonly id: SceneId
  /** The contract carries no leader card (a leader is a seat's look); match setup supplies it. */
  readonly leaders: readonly [CardRef, CardRef]
  /** What the contract does not carry yet about each seat's evolve points. */
  readonly points: Readonly<Record<Seat, ProvisionalPoints>>
  /** Everything each seat receives, in order. */
  readonly script: Readonly<Record<Seat, readonly HostMessage[]>>
}

const DRAGON_LEADER: CardNo = "BP10-LD05"
const ROYAL_LEADER: CardNo = "BP05-LD02"

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

const provisional = ({ epMax, sepMax, sepOpensIn }: SeatSpec): ProvisionalPoints => ({
  epMax,
  sepMax,
  sepOpensIn,
})

function scene(id: SceneId, table: TableSpec): Scene {
  return {
    id,
    leaders: [cardRef(DRAGON_LEADER), cardRef(ROYAL_LEADER)],
    points: { p1: provisional(table.seats[0]), p2: provisional(table.seats[1]) },
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
  epMax: 0,
  sepMax: 0,
  sepOpensIn: 0,
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
        epMax: 3,
        sepMax: 1,
        sepOpensIn: 4,
        deck: 23,
        hand: ["BP05-052", "BP15-057", "BP20-063", "BP05-058", "BP15-064", "BP20-067"],
        field: [
          { no: "BP20-062", slot: 1 },
          { no: "BP08-063", slot: 2, acted: true },
          { no: "BP20-064", slot: 3, keywords: ["guard"] },
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
        epMax: 0,
        sepMax: 1,
        sepOpensIn: 4,
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
        epMax: 0,
        sepMax: 1,
        sepOpensIn: 0,
        deck: 22,
        hand: ["BP05-052", "BP15-057", "BP20-063", "BP05-058", "BP15-064", "BP20-067"],
        field: [
          { no: "BP20-062", slot: 0 },
          { no: "BP08-063", slot: 1, acted: true },
          { no: "BP20-064", slot: 2, evolved: "BP20-065", acted: true, keywords: ["guard"] },
          { no: "BP20-062", slot: 3, stats: [3, 2] },
          { no: "BP20-067", slot: 4, counters: [["スタック", 2]] },
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
        sep: 0,
        epMax: 3,
        sepMax: 1,
        sepOpensIn: 0,
        deck: 22,
        hand: ["BP13-026", "BP16-021", "BP13-035", "BP16-026", "BP13-028", "BP21-022"],
        field: [
          { no: "BP13-030", slot: 0 },
          { no: "BP13-033", slot: 1, acted: true },
          { no: "BP18-023", slot: 2, stats: [5, 5], keywords: ["storm"] },
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
        epMax: 0,
        sepMax: 1,
        sepOpensIn: 0,
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
        epMax: 3,
        sepMax: 1,
        sepOpensIn: 0,
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
          { no: "SD07-016", slot: 4, keywords: ["guard"], counters: [["スタック", 3]] },
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
] as const satisfies readonly Scene[]
