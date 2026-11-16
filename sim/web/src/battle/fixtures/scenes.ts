// Fixture scenes: a described table per scene and what each seat is sent for it. Numbers, postures
// and keywords are arranged to exercise the screen, not to record a position a game reached.
import type {
  Answer,
  BattleInput,
  CardRef,
  HostError,
  HostMessage,
  Question,
  Seat,
} from "../contract"
import type { SlotArrangement } from "../view/table"
import { ASKED_SCENES, type AskedSceneId } from "./asked"
import { type FieldSpec, ready, type SeatSpec, type Step, type TableSpec, update } from "./build"
import { type CardNo, cardRef } from "./cards"
import { COMPOUND_SCENES, type CompoundSceneId } from "./compound"
import {
  DRAGON_EVOLVE,
  DRAGON_LEADER,
  ROYAL_EVOLVE,
  ROYAL_LEADER,
  SEP_FROM,
  WITCH_EVOLVE,
  WITCH_LEADER,
} from "./decks"

type SceneId =
  | "preset"
  | "opening"
  | "mid"
  | "full"
  | "stack"
  | "turn"
  | "quick"
  | "finished"
  | "fault"
  | "halted"
  | AskedSceneId
  | CompoundSceneId

export interface Scene {
  readonly id: SceneId
  /** The contract carries no leader card (a leader is a seat's look); match setup supplies it. */
  readonly leaders: readonly [CardRef, CardRef]
  /** Which slots the field cards lie in when the scene starts; the messages only give their order. */
  readonly slots: SlotArrangement
  /** Everything each seat receives when it starts, in order. */
  readonly script: Readonly<Record<Seat, readonly HostMessage[]>>
  /**
   * How the scene goes on: where it stands once these answers have been applied, in order, or null
   * when the script has no such course and the last answer is refused. A scene that only shows a
   * table refuses every answer.
   */
  readonly after: (answers: readonly Answer[]) => Step | null
}

/** The evolve deck with these cards taken out of it, one copy each. */
function without(deck: readonly CardNo[], taken: readonly CardNo[]): CardNo[] {
  const rest = [...deck]
  for (const no of taken) {
    const at = rest.indexOf(no)
    if (at !== -1) rest.splice(at, 1)
  }
  return rest
}

function scene(
  id: SceneId,
  table: TableSpec,
  leaders: readonly [CardNo, CardNo] = [DRAGON_LEADER, ROYAL_LEADER],
  course: {
    readonly after?: Scene["after"]
    /** What the first update says happens next, in place of the main-phase question. */
    readonly input?: BattleInput
    /** A host error that follows the first update. */
    readonly stop?: HostError
  } = {},
): Scene {
  const first = (seat: Seat): HostMessage[] => [
    ready(seat),
    update(table, seat, 0, course.input ?? "q0"),
    ...(course.stop === undefined ? [] : [course.stop]),
  ]
  return {
    id,
    leaders: [cardRef(leaders[0]), cardRef(leaders[1])],
    slots: {
      p1: { field: table.seats[0].field.map((card) => card.slot) },
      p2: { field: table.seats[1].field.map((card) => card.slot) },
    },
    script: { p1: first("p1"), p2: first("p2") },
    after: course.after ?? (() => null),
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

// One turn that can be played to its end, its actions in any order: play a follower, attack the
// leader or the engaged follower, end the turn. Every outcome is written out by hand, the way a
// player would lay the table after it; nothing here works out damage or what is legal. The cards
// that change zone carry a key, so each keeps its handle like an object of a real game.
interface TurnCourse {
  readonly played: boolean
  /** What the follower that has been on the field attacked. */
  readonly unifier: "ready" | "leader" | "fighter"
  /** The follower with rush arrived this turn: the engaged follower is all it may attack. */
  readonly priest: "ready" | "fighter"
  readonly ended: boolean
}

const TURN_START: TurnCourse = { played: false, unifier: "ready", priest: "ready", ended: false }

/** The course after one more answer, its options named as the first seat's question names them. */
function turnStep(course: TurnCourse, answer: Answer): TurnCourse | null {
  if (course.ended || answer.kind !== "action_window") return null
  const { intent, target } = answer
  if (intent === "play:a:fanatic") return { ...course, played: true }
  if (intent === "attack:a:unifier")
    return { ...course, unifier: target === "attack:a:unifier>leader" ? "leader" : "fighter" }
  if (intent === "attack:a:priest") return { ...course, priest: "fighter" }
  if (intent === "end") return { ...course, ended: true }
  return null
}

function turnTable(course: TurnCourse): TableSpec {
  // The engaged 3/2 falls to either attacker and deals its three damage back: the 3/4 is left
  // with one life, the 2/2 falls with it.
  const fallen = course.unifier === "fighter" || course.priest === "fighter"
  const reach = fallen ? [] : [1]
  const unifier: FieldSpec =
    course.unifier === "ready"
      ? {
          no: "BP20-062",
          slot: 0,
          key: "unifier",
          ...(course.ended ? {} : { attack: { leader: true, followers: reach } }),
        }
      : {
          no: "BP20-062",
          slot: 0,
          key: "unifier",
          acted: true,
          ...(course.unifier === "fighter" ? { stats: [3, 1] } : {}),
        }
  const priest: FieldSpec = {
    no: "BP20-064",
    slot: 1,
    key: "priest",
    keywords: ["rush"],
    ...(fallen || course.ended ? {} : { attack: { followers: reach } }),
  }
  const fanatic: FieldSpec = { no: "BP08-063", slot: 2, key: "fanatic" }
  const fighter: FieldSpec = { no: "BP13-033", slot: 1, key: "fighter", acted: !course.ended }
  return {
    turn: {
      active: course.ended ? "p2" : "p1",
      first_player: "p2",
      phase: "main",
      // The other seat begins its sixth turn: one more play point, all of them back, a card drawn,
      // and its own cards standing again.
      elapsed_turns: course.ended ? [5, 6] : [5, 5],
    },
    seats: [
      {
        life: 16,
        pp: course.played ? 1 : 3,
        maxPp: 5,
        ep: 2,
        sep: 1,
        epCapacity: 3,
        sepAvailableFrom: SEP_FROM.second,
        deck: 23,
        hand: [
          ...(course.played ? [] : [{ no: "BP08-063", key: "fanatic" } as const]),
          "BP05-052",
          "BP15-057",
        ],
        playable: course.played || course.ended ? [] : [0],
        field: [
          unifier,
          ...(course.priest === "fighter" ? [] : [priest]),
          ...(course.played ? [fanatic] : []),
        ],
        ex: ["BP20-T05"],
        cemetery: [
          "BP08-068",
          "SD04-002",
          ...(course.priest === "fighter" ? [{ no: "BP20-064", key: "priest" } as const] : []),
        ],
        banish: [],
        evolveDeck: { faceDown: DRAGON_EVOLVE, faceUp: [] },
      },
      {
        life: course.unifier === "leader" ? 15 : 18,
        pp: course.ended ? 6 : 1,
        maxPp: course.ended ? 6 : 5,
        ep: 0,
        sep: 1,
        epCapacity: 0,
        sepAvailableFrom: SEP_FROM.first,
        deck: course.ended ? 22 : 23,
        hand: [
          "BP13-026",
          "BP16-021",
          "BP13-035",
          "BP16-026",
          "BP13-028",
          ...(course.ended ? (["BP21-022"] as const) : []),
        ],
        field: [
          { no: "BP13-030", slot: 0 },
          ...(fallen ? [] : [fighter]),
          { no: "BP21-034", slot: 2 },
        ],
        ex: [],
        cemetery: [
          "SD07-012",
          "BP13-032",
          ...(fallen ? [{ no: "BP13-033", key: "fighter" } as const] : []),
        ],
        banish: [],
        evolveDeck: { faceDown: ROYAL_EVOLVE, faceUp: [] },
      },
    ],
  }
}

// The Quick spell of this scene looks at the deck's top cards and may take a spell from them. The
// script lets it find none, so playing it asks nothing further; the attack then lands either way.
function quickTable(used: boolean | null): TableSpec {
  const attacker = { no: "BP16-021", key: "attacker" } as const
  return {
    turn: { active: "p2", first_player: "p1", phase: "main", elapsed_turns: [4, 4] },
    ...(used === null ? { attack: { attacker, target: "p1" } } : {}),
    seats: [
      {
        life: used === null ? 18 : 14,
        pp: used === true ? 0 : 2,
        maxPp: 4,
        ep: 0,
        sep: 1,
        epCapacity: 0,
        sepAvailableFrom: SEP_FROM.first,
        deck: 27,
        hand: [
          ...(used === true ? [] : [{ no: "BP01-072", key: "quick" } as const]),
          "BP01-071",
          "BP01-054",
        ],
        field: [
          { no: "BP01-T10", slot: 0, counters: [["スタックカウンター", 3]] },
          { no: "BP01-067", slot: 1, acted: true },
        ],
        ex: [],
        cemetery: [
          "BP01-071",
          "BP01-068",
          ...(used === true ? [{ no: "BP01-072", key: "quick" } as const] : []),
        ],
        banish: [],
        evolveDeck: { faceDown: WITCH_EVOLVE, faceUp: [] },
      },
      {
        life: 17,
        pp: 1,
        maxPp: 4,
        ep: 3,
        sep: 1,
        epCapacity: 3,
        sepAvailableFrom: SEP_FROM.second,
        deck: 29,
        hand: ["BP13-026", "BP13-035", "BP16-026"],
        field: [
          { ...attacker, slot: 1, acted: true },
          { no: "BP13-026", slot: 3 },
        ],
        ex: ["BP16-026"],
        cemetery: ["SD07-012"],
        banish: [],
        evolveDeck: { faceDown: ROYAL_EVOLVE, faceUp: [] },
      },
    ],
  }
}

/** What the first seat is asked while the attack stands, worded like the contract's own sample. */
const QUICK_QUESTION: Question = {
  id: "quick",
  actor: "p1",
  purpose: "quick_window",
  use: "quick",
  processing: {
    stage: "quick",
    controller: "p1",
    source: null,
    subject: null,
    rule: null,
    procedure: null,
    declared: [],
    payment: null,
  },
  shape: {
    kind: "quick",
    window: "attack",
    options: [
      {
        id: "quick:a:quick",
        action: "play",
        source: {
          kind: "card",
          printing_id: cardRef("BP01-072").printing_id,
          face: 0,
          line: null,
          holder: "a:quick",
        },
        origin: { player: "p1", zone: "hand", position: 0 },
      },
    ],
  },
  rules: null,
  auto_answer: { kind: "not_unique" },
  assistance: null,
}

/** The table with the second seat's leader out of life. */
const lifeless = (table: TableSpec): TableSpec => ({
  ...table,
  seats: [table.seats[0], { ...table.seats[1], life: 0 }],
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
  scene("turn", turnTable(TURN_START), [DRAGON_LEADER, ROYAL_LEADER], {
    after: (answers) => {
      let course: TurnCourse | null = TURN_START
      for (const answer of answers) {
        course = turnStep(course, answer)
        if (course === null) return null
      }
      return { table: turnTable(course) }
    },
  }),
  // The other seat attacks the leader, and the first seat is asked whether to play its Quick spell
  // before the attack lands; either answer is taken once, then the turn goes on.
  scene("quick", quickTable(null), [WITCH_LEADER, ROYAL_LEADER], {
    input: { kind: "question", question: QUICK_QUESTION },
    after: ([answer, ...more]) =>
      answer?.kind !== "quick" || more.length > 0
        ? null
        : { table: quickTable(answer.option !== null) },
  }),
  // The game is over: nothing is asked of anybody.
  scene(
    "finished",
    lifeless(turnTable({ played: true, unifier: "leader", priest: "fighter", ended: false })),
    [DRAGON_LEADER, ROYAL_LEADER],
    { input: { kind: "finished", reason: "defeat", winner: "p1" } },
  ),
  // The engine stopped on a fault: the table stays as it last stood.
  scene(
    "fault",
    turnTable({ played: true, unifier: "fighter", priest: "ready", ended: false }),
    [DRAGON_LEADER, ROYAL_LEADER],
    { input: { kind: "engine_fault", can_concede: true } },
  ),
  // The host found its own messages inconsistent and stopped for good.
  scene("halted", turnTable(TURN_START), [DRAGON_LEADER, ROYAL_LEADER], {
    stop: { kind: "host_error", format: 1, reason: "projection" },
  }),
  ...[...ASKED_SCENES, ...COMPOUND_SCENES].map(({ id, table, leaders, input, after }) =>
    scene(id, table, leaders, { ...(input === undefined ? {} : { input }), after }),
  ),
] as const satisfies readonly Scene[]
