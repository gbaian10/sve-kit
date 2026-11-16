// Scenes for the questions that arrange things, ask for amounts, pay or answer a roll or a
// trigger. One is a real card's own: a follower evolves, the evolution is paid for, and what the
// evolved side does on evolving is answered. The rest are synthetic: no card of the fixture decks
// asks them, so each follows the contract's own sample of its kind and names no card as its
// source. Every outcome is written out by hand.
import type { Answer, BattleInput } from "../contract"
import { ask, type AskedScene, candidate, card, FOES, OWN, targetsTable } from "./asked"
import type { FieldSpec, SeatSpec, Step, TableSpec } from "./build"
import { type CardNo, fixtureCard } from "./cards"
import { DRAGON_EVOLVE, DRAGON_LEADER, ROYAL_LEADER } from "./decks"

export type CompoundSceneId =
  "order" | "partition" | "distribute" | "integer" | "dice" | "evolve" | "resource" | "drive"

type CompoundScene = AskedScene<CompoundSceneId>

const LEADERS = [DRAGON_LEADER, ROYAL_LEADER] as const

/** The table these scenes share: the first seat's main phase, two followers against three. */
const BASE = targetsTable(null)
const [SELF, OTHER] = BASE.seats

const table = (self: Partial<SeatSpec>, other: Partial<SeatSpec> = {}): TableSpec => ({
  ...BASE,
  seats: [
    { ...SELF, ...self },
    { ...OTHER, ...other },
  ],
})

/** A follower's numbers as printed, moved by what a scene does to it. */
function numbers(no: CardNo, attack: number, life: number): readonly [number, number] {
  const printed = fixtureCard(no).stats
  if (printed === null) throw new Error(`${no} has no attack and life`)
  return [printed[0] + attack, printed[1] + life]
}

/** The other seat's table after damage: a follower left without life lies on the cemetery. */
function damaged(dealt: (key: string | undefined) => number): Partial<SeatSpec> {
  const left = (spec: FieldSpec): number => numbers(spec.no, 0, -dealt(spec.key))[1]
  return {
    field: OTHER.field.flatMap((spec): FieldSpec[] => {
      if (dealt(spec.key) === 0) return [spec]
      return left(spec) <= 0 ? [] : [{ ...spec, stats: numbers(spec.no, 0, -dealt(spec.key)) }]
    }),
    cemetery: [
      ...OTHER.cemetery,
      ...OTHER.field
        .filter((spec) => dealt(spec.key) > 0 && left(spec) <= 0)
        .map(({ no, key }): SeatSpec["cemetery"][number] => (key === undefined ? no : { no, key })),
    ],
  }
}

/** A scene that asks once: any answer of its kind is taken, and the table after it is `then`. */
function once<K extends Answer["kind"]>(
  id: CompoundSceneId,
  kind: K,
  input: BattleInput,
  then: (answer: Extract<Answer, { kind: K }>) => Step | null,
): CompoundScene {
  return {
    id,
    table: BASE,
    leaders: LEADERS,
    input,
    after: ([answer, ...more]) =>
      answer?.kind !== kind || more.length > 0
        ? null
        : then(answer as Extract<Answer, { kind: K }>),
  }
}

// ---- Three cards looked at from the top of the deck: where each goes, and in which order.

const LOOKED = [
  { id: "top:1", no: "BP20-063", key: "looked-1" },
  { id: "top:2", no: "BP05-052", key: "looked-2" },
  { id: "top:3", no: "BP08-063", key: "looked-3" },
] as const
const looked = LOOKED.map((each) => candidate(each.id, each.no, null, "p1", "deck"))

const ORDER = once(
  "order",
  "order",
  ask("order", "order_cards", "resolution_choice", "resolve", {
    kind: "order",
    items: LOOKED.map((each) => ({ id: each.id, subject: card(each.no, null), source: null })),
    direction: "top_down",
    destination: { player: "p1", zone: "deck", position: "bottom" },
  }),
  // The cards go under the deck: nothing a seat can see changes.
  () => ({ table: BASE }),
)

const PARTITION = once(
  "partition",
  "partition",
  ask("partition", "search_partition", "resolution_choice", "resolve", {
    kind: "partition",
    items: looked,
    buckets: [
      {
        bucket: 0,
        destination: { player: "p1", zone: "hand", position: null },
        bounds: { min: 1, max: 1 },
        ordered: false,
      },
      {
        bucket: 1,
        destination: { player: "p1", zone: "deck", position: "bottom" },
        bounds: { min: 2, max: 2 },
        ordered: true,
      },
    ],
  }),
  (answer) => {
    const [kept] = answer.buckets.find((bucket) => bucket.bucket === 0)?.items ?? []
    const taken = LOOKED.find((each) => each.id === kept)
    return taken === undefined
      ? null
      : {
          table: table({
            hand: [...SELF.hand, { no: taken.no, key: taken.key }],
            deck: SELF.deck - 1,
          }),
        }
  },
)

// ---- Three damage dealt out among the other seat's followers, any amount to each.

const DISTRIBUTE = once(
  "distribute",
  "distribute",
  ask("distribute", "damage_distribution", "resolution_choice", "allocate", {
    kind: "distribute",
    recipients: FOES.map((foe, at) => candidate(foe.id, foe.no, foe.key, "p2", "field", at)),
    total: 3,
    share: { min: 0, max: null },
    empty: "allow_without_total",
  }),
  (answer) => ({
    table: table(
      {},
      damaged((key) => {
        const foe = FOES.find((each) => each.key === key)
        return answer.shares.find((share) => share.option === foe?.id)?.amount ?? 0
      }),
    ),
  }),
)

// ---- A number between one and three: that many cards are drawn.

const DRAWN = ["BP15-064", "BP20-067", "SD04-002"] as const

const INTEGER = once(
  "integer",
  "integer",
  ask("integer", "declare_number", "resolution_choice", "resolve", {
    kind: "integer",
    bounds: { min: 1, max: 3 },
    unit: "cards",
    source: null,
  }),
  (answer) => ({
    table: table({
      hand: [
        ...SELF.hand,
        ...DRAWN.slice(0, answer.value).map((no, at) => ({ no, key: `drawn-${String(at)}` })),
      ],
      deck: SELF.deck - answer.value,
    }),
  }),
)

// ---- A die that shows three, with one reroll left: kept, or rolled again and then kept.

const rolled = (id: string, value: number, remaining: number): BattleInput =>
  ask(id, "dice_reroll", "resolution_choice", "resolve", {
    kind: "dice_reroll",
    value,
    remaining,
    allowed: remaining > 0 ? ["keep", "reroll"] : ["keep"],
  })

const DICE: CompoundScene = {
  id: "dice",
  table: BASE,
  leaders: LEADERS,
  input: rolled("dice", 3, 1),
  after: ([first, second, ...more]) => {
    if (first?.kind !== "dice_reroll" || more.length > 0) return null
    if (first.choice === "keep") return second === undefined ? { table: BASE } : null
    // Rolled again: the die shows five and has to be kept.
    if (second === undefined) return { table: BASE, input: rolled("dice-again", 5, 0) }
    return second.kind === "dice_reroll" && second.choice === "keep" ? { table: BASE } : null
  },
}

// ---- The follower that may evolve does. The question that follows lists the ways to pay: the
// ---- one play point its text asks, or an evolve point in its place. Evolved, its other side
// ---- deals two damage to one of the other seat's followers.

const [, PRIEST] = OWN
const EVOLVED: CardNo = "BP20-065"
const evolving = table({
  field: SELF.field.map((spec): FieldSpec =>
    spec.key === PRIEST.key ? { ...spec, evolve: true } : spec,
  ),
})

function evolvedSeat(useEp: boolean): Partial<SeatSpec> {
  const taken = DRAGON_EVOLVE.indexOf(EVOLVED)
  return {
    pp: useEp ? SELF.pp : SELF.pp - 1,
    ep: useEp ? SELF.ep - 1 : SELF.ep,
    field: SELF.field.map((spec): FieldSpec =>
      spec.key === PRIEST.key ? { ...spec, evolved: EVOLVED } : spec,
    ),
    // The evolve card left the evolve deck for the top of its follower.
    evolveDeck: { faceDown: DRAGON_EVOLVE.filter((_, at) => at !== taken), faceUp: [] },
  }
}

const EVOLVE: CompoundScene = {
  id: "evolve",
  table: evolving,
  leaders: LEADERS,
  // It opens with the main-phase question, the evolution among its intents.
  after: ([intent, paid, hit, ...more]) => {
    if (intent?.kind !== "action_window" || more.length > 0) return null
    if (intent.intent !== `evolve:a:${PRIEST.key}`) return null
    if (paid === undefined)
      return {
        table: evolving,
        input: ask("evolution-payment", "evolution_payment", "play_parameter", "pay", {
          kind: "evolution_payment",
          payments: [
            { pp: 1, use_ep: false, super_evolution: false },
            { pp: 0, use_ep: true, super_evolution: false },
          ],
        }),
      }
    if (paid.kind !== "evolution_payment") return null
    const self = evolvedSeat(paid.use_ep)
    if (hit === undefined)
      return {
        table: table(self),
        input: ask("on-evolve", "resolution_selection", "resolution_choice", "resolve", {
          kind: "select",
          selection: {
            bounds: { min: 1, max: 1 },
            candidates: FOES.map((foe, at) =>
              candidate(foe.id, foe.no, foe.key, "p2", "field", at),
            ),
            capacity: null,
            distinct: [],
            group_assignment: "existence",
            groups: [],
            original_cost_total: { min: null, max: null },
            roles: [],
            shown: [],
          },
        }),
      }
    const foe = hit.kind === "select" ? FOES.find((each) => each.id === hit.selected[0]) : undefined
    return foe === undefined
      ? null
      : {
          table: table(
            self,
            damaged((key) => (key === foe.key ? 2 : 0)),
          ),
        }
  },
}

// ---- A cost paid by engaging up to two of the seat's own followers, with an evolve point
// ---- allowed in place of a play point.

const RESOURCE = once(
  "resource",
  "resource_payment",
  ask("resource", "resource_entry_payment", "play_parameter", "pay", {
    kind: "resource_payment",
    materials: {
      bounds: { min: 0, max: 2 },
      candidates: OWN.map((own, at) => candidate(own.id, own.no, own.key, "p1", "field", at)),
      capacity: null,
      distinct: [],
      group_assignment: "existence",
      groups: [],
      original_cost_total: { min: null, max: null },
      roles: [],
      shown: [],
    },
    ep_allowed: true,
  }),
  (answer) => ({
    table: table({
      pp: answer.use_ep ? SELF.pp : SELF.pp - 1,
      ep: answer.use_ep ? SELF.ep - 1 : SELF.ep,
      field: SELF.field.map((spec): FieldSpec => {
        const own = OWN.find((each) => each.key === spec.key)
        return own !== undefined && answer.materials.includes(own.id)
          ? { ...spec, acted: true }
          : spec
      }),
    }),
  }),
)

// ---- A drive trigger that may be accepted for one of the seat's own followers, or declined.
// ---- Accepted, the follower it names gains one attack.

const DRIVE = once(
  "drive",
  "drive_trigger",
  ask("drive", "drive_trigger", "rule_choice", "resolve", {
    kind: "drive_trigger",
    icon: "critical",
    allowed: [true, false],
    targets: OWN.map((own, at) => candidate(own.id, own.no, own.key, "p1", "field", at)),
    target_bounds: { min: 1, max: 1 },
  }),
  (answer) => ({
    table: table({
      field: SELF.field.map((spec): FieldSpec => {
        const own = OWN.find((each) => each.key === spec.key)
        if (!answer.accept || own === undefined || own.id !== answer.target) return spec
        return { ...spec, stats: numbers(own.no, 1, 0) }
      }),
    }),
  }),
)

export const COMPOUND_SCENES: readonly CompoundScene[] = [
  ORDER,
  PARTITION,
  DISTRIBUTE,
  INTEGER,
  DICE,
  EVOLVE,
  RESOURCE,
  DRIVE,
]
