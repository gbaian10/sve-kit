// Scenes that put one of the selecting questions to the first seat: a selection, a choice, the
// opening hand, a yes or no. Each is one effect of a real card of the fixture decks (or a step of
// the opening), taken as far as its question and the table after the answer. Every outcome is
// written out by hand; nothing here works out damage or what is legal. The card being played
// stays in the hand while its question is open: these tables do not lay out the resolution area.
import type {
  Answer,
  BattleInput,
  Candidate,
  Question,
  QuestionShape,
  Seat,
  Subject,
  Zone,
} from "../contract"
import type { FieldSpec, SeatSpec, Step, TableSpec } from "./build"
import { type CardNo, cardRef, fixtureCard } from "./cards"
import { DRAGON_EVOLVE, DRAGON_LEADER, ROYAL_EVOLVE, ROYAL_LEADER, SEP_FROM } from "./decks"

export type AskedSceneId = "targets" | "sisters" | "discard" | "modes" | "firstPlayer" | "mulligan"

export interface AskedScene<Id extends string = AskedSceneId> {
  readonly id: Id
  readonly table: TableSpec
  readonly leaders: readonly [CardNo, CardNo]
  /** What the scene opens with; without one, the main-phase question its table describes. */
  readonly input?: BattleInput
  readonly after: (answers: readonly Answer[]) => Step | null
}

/** One question put to the first seat, worded like the contract's own samples. */
export function ask(
  id: string,
  use: Question["use"],
  purpose: Question["purpose"],
  stage: Question["processing"]["stage"],
  shape: QuestionShape,
): BattleInput {
  return {
    kind: "question",
    question: {
      id,
      actor: "p1",
      purpose,
      use,
      processing: {
        stage,
        actor: "p1",
        controller: "p1",
        source: null,
        subject: null,
        rule: null,
        procedure: null,
        declared: [],
        payments: [],
      },
      shape,
      rules: null,
      auto_answer: { kind: "not_unique" },
      assistance: null,
    },
  }
}

/** The first seat's handle of a keyed card. */
const handle = (key: string): string => `a:${key}`

export const card = (no: CardNo, key: string | null): Subject => ({
  kind: "card",
  card: cardRef(no),
  object: key === null ? null : handle(key),
})

/** A card as a candidate: where it is, and the name the question tells apart by. */
export function candidate(
  id: string,
  no: CardNo,
  key: string | null,
  player: Seat,
  zone: Zone,
  position: number | null = null,
): Candidate {
  return {
    id,
    subject: card(no, key),
    controller: player,
    origin: { player, zone, position },
    rules_names: [fixtureCard(no).name],
    original_cost: fixtureCard(no).cost,
    classes: null,
  }
}

const MAIN = { phase: "main", first_player: "p2" } as const

/** What a selection answered, when the answer is one. */
const picked = (answer: Answer | undefined): readonly string[] | null =>
  answer?.kind === "select" ? answer.selected : null

// ---- A spell that needs one follower of each side: 1 damage to the seat's own, 3 to the other's.

const CLAW = { no: "BP05-058", key: "claw" } as const
export const OWN = [
  { id: "own:unifier", no: "BP20-062", key: "unifier", hurt: [3, 3] },
  { id: "own:priest", no: "BP20-064", key: "priest", hurt: [2, 1] },
] as const
// Each of these has three life or less: whichever is chosen falls.
export const FOES = [
  { id: "foe:maimu", no: "BP13-030", key: "maimu", acted: false },
  { id: "foe:meimu", no: "BP13-033", key: "meimu", acted: true },
  { id: "foe:archer", no: "BP21-034", key: "archer", acted: false },
] as const

export function targetsTable(
  hit: { readonly own: string; readonly foe: string } | null,
): TableSpec {
  return {
    turn: { ...MAIN, active: "p1", elapsed_turns: [5, 5] },
    seats: [
      {
        life: 16,
        pp: hit === null ? 3 : 2,
        maxPp: 5,
        ep: 2,
        sep: 1,
        epCapacity: 3,
        sepAvailableFrom: SEP_FROM.second,
        deck: 23,
        hand: [...(hit === null ? [CLAW] : []), "BP05-052", "BP15-057"],
        field: OWN.map((own, slot): FieldSpec => ({
          no: own.no,
          key: own.key,
          slot,
          ...(hit?.own === own.id ? { stats: own.hurt } : {}),
        })),
        ex: [],
        cemetery: ["BP08-068", ...(hit === null ? [] : [CLAW])],
        banish: [],
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
        field: FOES.filter((foe) => foe.id !== hit?.foe).map((foe): FieldSpec => ({
          no: foe.no,
          key: foe.key,
          slot: FOES.indexOf(foe),
          ...(foe.acted ? { acted: true } : {}),
        })),
        ex: [],
        cemetery: [
          "SD07-012",
          ...FOES.filter((foe) => foe.id === hit?.foe).map((foe) => ({ no: foe.no, key: foe.key })),
        ],
        banish: [],
        evolveDeck: { faceDown: ROYAL_EVOLVE, faceUp: [] },
      },
    ],
  }
}

const TARGETS: AskedScene = {
  id: "targets",
  table: targetsTable(null),
  leaders: [DRAGON_LEADER, ROYAL_LEADER],
  // One of each side: two groups whose candidates do not overlap, so the answer need not say
  // which pick belongs to which.
  input: ask("targets", "targets", "play_parameter", "target", {
    kind: "select",
    selection: {
      bounds: { min: 2, max: 2 },
      candidates: [
        ...OWN.map((own, at) => candidate(own.id, own.no, own.key, "p1", "field", at)),
        ...FOES.map((foe, at) => candidate(foe.id, foe.no, foe.key, "p2", "field", at)),
      ],
      capacity: null,
      distinct: [],
      group_assignment: "existence",
      groups: [
        { group: 0, eligible: OWN.map((own) => own.id), bounds: { min: 1, max: 1 } },
        { group: 1, eligible: FOES.map((foe) => foe.id), bounds: { min: 1, max: 1 } },
      ],
      original_cost_total: { min: null, max: null },
      roles: [],
      shown: [],
    },
  }),
  after: ([answer, ...more]) => {
    const chosen = picked(answer)
    const own = OWN.find((each) => chosen?.includes(each.id))
    const foe = FOES.find((each) => chosen?.includes(each.id))
    return more.length > 0 || own === undefined || foe === undefined
      ? null
      : { table: targetsTable({ own: own.id, foe: foe.id }) }
  },
}

// ---- A spell that may be played for four more: for its own cost it searches one of three named
// ---- followers into the hand, for the higher cost one of each onto the field.

const SISTERS = { no: "BP13-035", key: "sisters" } as const
// What the deck holds of the three names: two of the first, one of the second, none of the third.
const FOUND = [
  { id: "deck:maimu-1", no: "BP13-030", key: "found-maimu-1", name: 0 },
  { id: "deck:maimu-2", no: "BP13-030", key: "found-maimu-2", name: 0 },
  { id: "deck:miimu", no: "BP13-032", key: "found-miimu", name: 1 },
] as const
const NAMES = 3

function sistersTable(paid: boolean | null, found: readonly string[] | null): TableSpec {
  const taken = FOUND.filter((each) => found?.includes(each.id))
  return {
    turn: { phase: "main", first_player: "p1", active: "p1", elapsed_turns: [6, 5] },
    seats: [
      {
        life: 17,
        pp: found === null ? 6 : paid === true ? 1 : 5,
        maxPp: 6,
        ep: 0,
        sep: 1,
        epCapacity: 0,
        sepAvailableFrom: SEP_FROM.first,
        deck: 24 - taken.length,
        hand: [
          ...(found === null ? [SISTERS] : []),
          "BP13-026",
          "BP16-026",
          ...(paid === false ? taken.map(({ no, key }) => ({ no, key })) : []),
        ],
        field: [
          { no: "BP16-021", slot: 0 },
          { no: "BP13-028", slot: 1 },
          ...(paid === true
            ? taken.map(({ no, key }, at): FieldSpec => ({ no, key, slot: 2 + at }))
            : []),
        ],
        ex: [],
        cemetery: ["SD07-012", ...(found === null ? [] : [SISTERS])],
        banish: [],
        evolveDeck: { faceDown: ROYAL_EVOLVE, faceUp: [] },
      },
      {
        life: 16,
        pp: 0,
        maxPp: 5,
        ep: 2,
        sep: 1,
        epCapacity: 3,
        sepAvailableFrom: SEP_FROM.second,
        deck: 24,
        hand: ["BP05-052", "BP15-057", "BP20-063"],
        field: [
          { no: "BP20-062", slot: 0 },
          { no: "BP20-064", slot: 1, acted: true },
        ],
        ex: [],
        cemetery: ["BP08-068"],
        banish: [],
        evolveDeck: { faceDown: DRAGON_EVOLVE, faceUp: [] },
      },
    ],
  }
}

/** The search that follows: one card into the hand, or one of each name onto the field. */
function sistersSearch(paid: boolean): BattleInput {
  const candidates = FOUND.map((each) => candidate(each.id, each.no, null, "p1", "deck"))
  return ask("search", "search", "resolution_choice", "resolve", {
    kind: "select",
    selection: paid
      ? {
          bounds: { min: 0, max: NAMES },
          candidates,
          // Three arrivals at most, and the field has room for three.
          capacity: { available: 3, destination: { player: "p1", zone: "field", position: null } },
          distinct: ["rules_name"],
          // One of each name, and the answer says which pick stands for which name. The deck
          // holds none of the third name: its group is there and stays empty.
          group_assignment: "explicit",
          groups: Array.from({ length: NAMES }, (_, name) => {
            const eligible = FOUND.filter((each) => each.name === name).map((each) => each.id)
            return { group: name, eligible, bounds: { min: 0, max: Math.min(1, eligible.length) } }
          }),
          original_cost_total: { min: null, max: null },
          roles: [],
          // The rest of the deck is looked through as well; two of its cards stand for it here.
          shown: [card("BP16-021", null), card("BP13-026", null)],
        }
      : {
          bounds: { min: 0, max: 1 },
          candidates,
          capacity: null,
          distinct: [],
          group_assignment: "existence",
          groups: [],
          original_cost_total: { min: null, max: null },
          roles: [],
          shown: [card("BP16-021", null), card("BP13-026", null)],
        },
  })
}

const SISTERS_SCENE: AskedScene = {
  id: "sisters",
  table: sistersTable(null, null),
  leaders: [ROYAL_LEADER, DRAGON_LEADER],
  input: ask("extra-cost", "optional_cost", "play_parameter", "declare", {
    kind: "confirm",
    allowed: [true, false],
    subject: card(SISTERS.no, SISTERS.key),
    origin: { player: "p1", zone: "hand", position: 0 },
  }),
  after: ([first, second, ...more]) => {
    if (first?.kind !== "confirm" || more.length > 0) return null
    if (second === undefined)
      return { table: sistersTable(null, null), input: sistersSearch(first.accept) }
    const found = picked(second)
    return found === null ? null : { table: sistersTable(first.accept, found) }
  },
}

// ---- Nine cards in hand at the end of the turn: two are discarded, in the order they are named.

const HELD = [
  "BP05-052",
  "BP15-057",
  "BP20-063",
  "BP05-058",
  "BP15-064",
  "BP20-067",
  "BP08-063",
  "BP20-062",
  "SD04-002",
] as const satisfies readonly CardNo[]
const held = (at: number) => ({ no: HELD[at] ?? HELD[0], key: `held-${String(at)}` })
const HAND = HELD.map((_, at) => held(at))

function discardTable(gone: readonly string[] | null): TableSpec {
  const discarded = (gone ?? []).flatMap((id) => HAND.filter((each) => each.key === id))
  const other: SeatSpec = {
    life: 15,
    pp: gone === null ? 2 : 8,
    maxPp: gone === null ? 7 : 8,
    ep: 0,
    sep: 0,
    epCapacity: 0,
    sepAvailableFrom: SEP_FROM.first,
    deck: gone === null ? 18 : 17,
    hand: ["BP13-026", "BP16-021", "BP13-035", ...(gone === null ? [] : (["BP13-028"] as const))],
    field: [{ no: "BP13-030", slot: 0 }],
    ex: [],
    cemetery: ["SD07-012", "BP13-032"],
    banish: [],
    evolveDeck: { faceDown: ROYAL_EVOLVE, faceUp: [] },
  }
  return {
    // Once the hand is back to its limit the turn is over and the other seat's begins.
    turn:
      gone === null
        ? { active: "p1", first_player: "p2", phase: "end", elapsed_turns: [7, 7] }
        : { active: "p2", first_player: "p2", phase: "main", elapsed_turns: [7, 8] },
    seats: [
      {
        life: 14,
        pp: 0,
        maxPp: 7,
        ep: 1,
        sep: 1,
        epCapacity: 3,
        sepAvailableFrom: SEP_FROM.second,
        deck: 16,
        hand: HAND.filter((each) => !discarded.includes(each)),
        field: [{ no: "BP20-064", slot: 0, acted: true }],
        ex: [],
        cemetery: ["BP08-068", ...discarded],
        banish: [],
        evolveDeck: { faceDown: DRAGON_EVOLVE, faceUp: [] },
      },
      other,
    ],
  }
}

const DISCARD: AskedScene = {
  id: "discard",
  table: discardTable(null),
  leaders: [DRAGON_LEADER, ROYAL_LEADER],
  input: ask("discard", "hand_limit_discard", "rule_choice", "closing", {
    kind: "select",
    selection: {
      bounds: { min: 2, max: 2 },
      candidates: HAND.map((each, at) => candidate(each.key, each.no, each.key, "p1", "hand", at)),
      capacity: null,
      distinct: [],
      group_assignment: "existence",
      groups: [],
      original_cost_total: { min: null, max: null },
      roles: [],
      shown: [],
    },
  }),
  after: ([answer, ...more]) => {
    const gone = picked(answer)
    return more.length > 0 || gone === null ? null : { table: discardTable(gone) }
  },
}

// ---- A spell with two modes: one more maximum play point, or a card drawn.

const ORACLE = { no: "SD04-002", key: "oracle" } as const

function modesTable(mode: 1 | 2 | null): TableSpec {
  const first = targetsTable(null).seats
  return {
    turn: { ...MAIN, active: "p1", elapsed_turns: [5, 5] },
    seats: [
      {
        ...first[0],
        pp: mode === null ? 5 : 3,
        maxPp: mode === 1 ? 6 : 5,
        deck: mode === 2 ? 22 : 23,
        hand: [
          ...(mode === null ? [ORACLE] : []),
          "BP05-052",
          "BP15-057",
          ...(mode === 2 ? [{ no: "BP20-063", key: "drawn" } as const] : []),
        ],
        cemetery: ["BP08-068", ...(mode === null ? [] : [ORACLE])],
      },
      first[1],
    ],
  }
}

const MODES: AskedScene = {
  id: "modes",
  table: modesTable(null),
  leaders: [DRAGON_LEADER, ROYAL_LEADER],
  input: ask("modes", "modes", "play_parameter", "declare", {
    kind: "choose",
    options: [
      { id: "mode:1", value: { kind: "mode", mode: 1, text: null }, cost: null },
      { id: "mode:2", value: { kind: "mode", mode: 2, text: null }, cost: null },
    ],
    bounds: { min: 1, max: 1 },
    count: 1,
    earlier: [],
    capacity: null,
  }),
  after: ([answer, ...more]) => {
    if (answer?.kind !== "choose" || more.length > 0) return null
    const [chosen] = answer.chosen
    return { table: modesTable(chosen === "mode:1" ? 1 : 2) }
  },
}

// ---- The opening: who plays first, then the hand each seat starts with.

const OPENING_HAND = [
  "BP20-064",
  "BP05-058",
  "BP20-062",
  "BP15-057",
] as const satisfies readonly CardNo[]
const REDRAWN = [
  "BP08-063",
  "BP20-063",
  "BP05-052",
  "SD04-002",
] as const satisfies readonly CardNo[]

function openingTable(first: Seat | null, redrawn: boolean): TableSpec {
  const seat = (hand: SeatSpec["hand"], evolve: readonly CardNo[]): SeatSpec => ({
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
  return {
    // Until the first player is chosen no seat is the active one; once it is, that seat is, and
    // the phase stays open for as long as the opening lasts.
    turn: { active: first, first_player: first, phase: null, elapsed_turns: [0, 0] },
    seats: [
      seat(
        (redrawn ? REDRAWN : OPENING_HAND).map((no, at) => ({
          no,
          key: `${redrawn ? "redrawn" : "opening"}-${String(at)}`,
        })),
        DRAGON_EVOLVE,
      ),
      seat(["BP13-026", "BP13-030", "BP16-026", "BP16-021"], ROYAL_EVOLVE),
    ],
  }
}

/** The other seat is the next to decide: this seat waits for it. */
const waitingForOther: BattleInput = {
  kind: "waiting",
  actor: "p2",
  context: { processing: null, dice: null },
}

const FIRST_PLAYER: AskedScene = {
  id: "firstPlayer",
  table: openingTable(null, false),
  leaders: [DRAGON_LEADER, ROYAL_LEADER],
  input: ask("first-player", "first_player", "rule_choice", "opening", {
    kind: "choose",
    options: [
      { id: "seat:p1", value: { kind: "seat", player: "p1" }, cost: null },
      { id: "seat:p2", value: { kind: "seat", player: "p2" }, cost: null },
    ],
    bounds: { min: 1, max: 1 },
    count: null,
    earlier: [],
    capacity: null,
  }),
  after: ([answer, ...more]) => {
    if (answer?.kind !== "choose" || more.length > 0) return null
    const [chosen] = answer.chosen
    return {
      table: openingTable(chosen === "seat:p1" ? "p1" : "p2", false),
      input: waitingForOther,
    }
  },
}

const MULLIGAN: AskedScene = {
  id: "mulligan",
  table: openingTable("p1", false),
  leaders: [DRAGON_LEADER, ROYAL_LEADER],
  input: ask("mulligan", "mulligan", "rule_choice", "opening", {
    kind: "mulligan",
    hand: OPENING_HAND.map((no, at) => ({
      ...candidate(`hand:${String(at)}`, no, `opening-${String(at)}`, "p1", "hand", at),
      controller: null,
      rules_names: null,
      original_cost: null,
    })),
  }),
  after: ([answer, ...more]) =>
    answer?.kind !== "mulligan" || more.length > 0
      ? null
      : { table: openingTable("p1", answer.redraw), input: waitingForOther },
}

export const ASKED_SCENES: readonly AskedScene[] = [
  TARGETS,
  SISTERS_SCENE,
  DISCARD,
  MODES,
  FIRST_PLAYER,
  MULLIGAN,
]
