// @vitest-environment node
import { describe, expect, it } from "vitest"

import { type MainIntent, type Question, type SourceRef, validateClientMessage } from "../contract"
import { update } from "../fixtures/build"
import { SCENES } from "../fixtures/scenes"
import { act, actions } from "./actions"

const ABILITY: SourceRef = {
  kind: "card",
  printing_id: "p:x",
  face: null,
  holder: null,
  line: null,
}

const INTENTS: MainIntent[] = [
  {
    kind: "play",
    id: "o1",
    source: "h1",
    cost: { basis: "current_context", total_pp: 2, groups: [], pending: [] },
  },
  // A second way to play the same card, and a play whose price is still open.
  { kind: "play", id: "o2", source: "h1", cost: null },
  {
    kind: "play",
    id: "o3",
    source: "h2",
    cost: { basis: "current_context", total_pp: null, groups: [], pending: [] },
  },
  { kind: "activate", id: "o4", holder: "f1", ability: ABILITY },
  { kind: "activate", id: "o5", holder: "f1", ability: ABILITY },
  { kind: "evolve", id: "o6", holder: "f2", ability: ABILITY, hint: null },
  {
    kind: "attack",
    id: "o7",
    attacker: "f2",
    targets: [
      { id: "t1", target: { kind: "card", object: "e1" } },
      { id: "t2", target: { kind: "leader", player: "p2" } },
    ],
  },
  {
    kind: "attack",
    id: "o8",
    attacker: "f3",
    targets: [{ id: "t3", target: { kind: "card", object: "e2" } }],
  },
  { kind: "end_phase", id: "o9" },
]

/** A main-phase question as the fixtures word it, with the intents above. */
function question(intents: MainIntent[] = INTENTS): Question {
  const { input } = SCENES[0].script.p1.find((message) => message.kind === "update") ?? {}
  if (input?.kind !== "question") throw new Error("the first scene asks its first seat")
  return { ...input.question, id: "q7", shape: { kind: "action_window", intents } }
}

describe("actions", () => {
  it("reads every intent of a main-phase question, by the card it belongs to", () => {
    const acts = actions(question(), "p1")
    expect(acts?.question).toBe("q7")
    expect([...(acts?.play ?? [])]).toEqual([
      [
        "h1",
        [
          { option: "o1", cost: 2 },
          { option: "o2", cost: null },
        ],
      ],
      ["h2", [{ option: "o3", cost: null }]],
    ])
    expect([...(acts?.activate ?? [])]).toEqual([["f1", ["o4", "o5"]]])
    expect([...(acts?.evolve ?? [])]).toEqual([["f2", ["o6"]]])
    expect(acts?.endPhase).toBe("o9")
  })

  it("gives each attacker its own recipients", () => {
    const acts = actions(question(), "p1")
    const both = acts?.attack.get("f2")
    expect(both).toMatchObject({ option: "o7", leader: "t2" })
    expect([...(both?.cards ?? [])]).toEqual([["e1", "t1"]])
    const cardsOnly = acts?.attack.get("f3")
    expect(cardsOnly).toMatchObject({ option: "o8", leader: null })
    expect([...(cardsOnly?.cards ?? [])]).toEqual([["e2", "t3"]])
    // A card the question names nowhere may do nothing.
    expect(acts?.attack.get("f1")).toBeUndefined()
  })

  it("offers no end of the phase the question does not list", () => {
    const acts = actions(question(INTENTS.slice(0, -1)), "p1")
    expect(acts?.endPhase).toBeNull()
  })

  it("is nothing without a main-phase question put to this seat", () => {
    expect(actions(null, "p1")).toBeNull()
    // Another seat's question says nothing about what this seat may do.
    expect(actions(question(), "p2")).toBeNull()
    // Other kinds of question are not answered from the table.
    const quick: Question = {
      ...question(),
      shape: { kind: "quick", window: "attack", options: [] },
    }
    expect(actions(quick, "p1")).toBeNull()
  })

  it("answers with one intent, and with the recipient for an attack", () => {
    const acts = actions(question(), "p1")
    if (acts === null) throw new Error("actions expected")
    expect(act(acts, "o9")).toEqual({
      kind: "answer",
      question_id: "q7",
      answer: { kind: "action_window", intent: "o9", target: null },
    })
    const attack = act(acts, "o7", "t2")
    expect(attack).toMatchObject({ answer: { intent: "o7", target: "t2" } })
    expect(validateClientMessage({ kind: "submit", request_id: "r1", command: attack })).toBe(true)
  })

  it("reads the fixtures' own question the same way", () => {
    const [first, second] = SCENES[0].script.p1
      .filter((message) => message.kind === "update")
      .map((message) => message.observation.players)
      .flat()
    if (first === undefined || second === undefined) throw new Error("two seats expected")
    const { input } = update(
      {
        turn: { active: "p1", first_player: "p1", phase: "main", elapsed_turns: [1, 0] },
        seats: [
          {
            life: 20,
            pp: 2,
            maxPp: 2,
            ep: 0,
            sep: 0,
            epCapacity: 0,
            sepAvailableFrom: null,
            deck: 30,
            hand: ["BP08-063", "BP05-052"],
            playable: [0],
            field: [{ no: "BP20-062", slot: 0, attack: { leader: true, followers: [0] } }],
            ex: [],
            cemetery: [],
            banish: [],
            evolveDeck: { faceDown: [], faceUp: [] },
          },
          {
            life: 20,
            pp: 0,
            maxPp: 1,
            ep: 0,
            sep: 0,
            epCapacity: 0,
            sepAvailableFrom: null,
            deck: 30,
            hand: [],
            field: [{ no: "BP13-033", slot: 0, acted: true }],
            ex: [],
            cemetery: [],
            banish: [],
            evolveDeck: { faceDown: [], faceUp: [] },
          },
        ],
      },
      "p1",
      0,
      "q0",
    )
    if (input.kind !== "question") throw new Error("the active seat is asked")
    const acts = actions(input.question, "p1")
    // One playable hand card at its quoted price, one attacker with both kinds of recipient.
    expect([...(acts?.play.values() ?? [])].flat().map((play) => play.cost)).toEqual([2])
    const [attack] = [...(acts?.attack.values() ?? [])]
    expect(attack?.leader).not.toBeNull()
    expect(attack?.cards.size).toBe(1)
    expect(acts?.endPhase).not.toBeNull()
  })
})
