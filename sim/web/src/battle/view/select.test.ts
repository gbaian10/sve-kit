// @vitest-environment node
import { readFileSync } from "node:fs"
import path from "node:path"

import { describe, expect, it } from "vitest"

import { type Question, validateClientMessage, validateQuestion } from "../contract"
import {
  costTotal,
  give,
  NO_PICKS,
  onTable,
  type Picks,
  problems,
  selectAnswer,
  selection,
  toggle,
} from "./select"

/** One of the contract's own sample questions. */
function sample(name: string): Question {
  const file = path.resolve(
    import.meta.dirname,
    "../../../../battle-contract/fixtures/question",
    `${name}.json`,
  )
  const question: unknown = JSON.parse(readFileSync(file, "utf8"))
  if (!validateQuestion(question)) throw new Error(`${name} is not a question`)
  return question
}

function asked(name: string) {
  const question = sample(name)
  const view = selection(question, question.actor)
  if (view === null) throw new Error(`${name} is no selection`)
  return view
}

const pick = (view: ReturnType<typeof asked>, ...options: string[]): Picks =>
  options.reduce((picks, option) => toggle(view, picks, option), NO_PICKS)

const kinds = (view: ReturnType<typeof asked>, picks: Picks): string[] =>
  problems(view, picks).map((problem) => problem.kind)

describe("selection", () => {
  it("reads a selection put to this seat, and nothing else", () => {
    const question = sample("targets")
    expect(selection(question, "p1")?.question).toBe(question.id)
    expect(selection(question, "p2")).toBeNull()
    expect(selection(sample("modes"), "p1")).toBeNull()
    expect(selection(null, "p1")).toBeNull()
  })

  it("says where a subject is to be found on the table", () => {
    const card = { printing_id: "p", face: 0 }
    expect(onTable({ kind: "card", card, object: "o1" })).toEqual({ kind: "card", handle: "o1" })
    // A card the seat knows but cannot point at, such as one inside a deck.
    expect(onTable({ kind: "card", card, object: null })).toBeNull()
    expect(onTable({ kind: "leader", player: "p2" })).toEqual({ kind: "leader", seat: "p2" })
    expect(onTable({ kind: "player", player: "p1" })).toEqual({ kind: "leader", seat: "p1" })
    expect(onTable(null)).toBeNull()
  })
})

describe("toggle", () => {
  it("picks in order, puts a pick back, and takes no more than the question's maximum", () => {
    const view = asked("targets")
    expect(pick(view, "c2", "c1").selected).toEqual(["c2", "c1"])
    // Two at most: a third is not taken, and a candidate the question does not list never is.
    expect(pick(view, "c2", "c1", "c3").selected).toEqual(["c2", "c1"])
    expect(pick(view, "c2", "nothing").selected).toEqual(["c2"])
    expect(pick(view, "c2", "c1", "c2").selected).toEqual(["c1"])
  })

  it("swaps the pick of a question that takes a single one", () => {
    const view = asked("hand-limit-discard")
    const [first, second] = view.selection.candidates
    if (first === undefined || second === undefined) throw new Error("two candidates expected")
    expect(pick(view, first.id, second.id).selected).toEqual([second.id])
  })

  it("gives a new pick the first group that accepts it, where groups are to be named", () => {
    const view = asked("search")
    // c2 is accepted by both groups, c3 by the second only.
    const picks = pick(view, "c2", "c3")
    expect([...picks.groups]).toEqual([
      ["c2", 0],
      ["c3", 1],
    ])
    // Put back, a pick keeps no group.
    expect([...toggle(view, picks, "c2").groups]).toEqual([["c3", 1]])
    // Where the answer names no groups, none is kept.
    expect(pick(asked("targets"), "c1").groups.size).toBe(0)
  })
})

describe("problems", () => {
  it("counts the picks against the question's bounds", () => {
    const view = asked("targets")
    expect(problems(view, NO_PICKS)).toContainEqual({
      kind: "count",
      have: 0,
      bounds: { min: 1, max: 2 },
    })
    // Within the bounds, the count has nothing to say; the roles are sized on their own.
    expect(kinds(view, pick(view, "c1"))).not.toContain("count")
  })

  it("counts each named group, and wants every pick in a group that accepts it", () => {
    const view = asked("search")
    // Both picks went to their first group: c1 and c2 share the first, which takes one.
    const crowded = pick(view, "c1", "c2")
    expect(problems(view, crowded)).toContainEqual({
      kind: "group",
      group: 0,
      have: 2,
      bounds: { min: 0, max: 1 },
    })
    // Moved to the second group, which accepts it, the count is right.
    const moved = give(crowded, "c2", { group: 1 })
    expect(kinds(view, moved).filter((kind) => kind === "group" || kind === "ungrouped")).toEqual(
      [],
    )
    // The second group does not accept c1.
    expect(problems(view, give(pick(view, "c1"), "c1", { group: 1 }))).toContainEqual({
      kind: "ungrouped",
      option: "c1",
    })
  })

  it("leaves groups alone where the answer does not name them", () => {
    // Whether an assignment exists is the authority's to work out, not a count to compare.
    const view = asked("targets")
    expect(kinds(view, give(pick(view, "c1"), "c1", { role: 0 }))).not.toContain("group")
  })

  it("sizes the roles: fixed, only for a selection that is not empty, and the remainder", () => {
    const view = asked("targets")
    // Role 0 takes exactly one, role 1 exactly one once anything is picked, role 2 the rest.
    const none = pick(view, "c1", "c2")
    expect(problems(view, none).filter((problem) => problem.kind === "role")).toEqual([
      { kind: "role", role: 0, have: 0, bounds: { min: 1, max: 1 } },
      { kind: "role", role: 1, have: 0, bounds: { min: 1, max: 1 } },
    ])
    const split = give(give(none, "c1", { role: 0 }), "c2", { role: 1 })
    expect(kinds(view, split)).toEqual([])
    // Nothing picked: the role that exists only for a selection takes nothing.
    expect(problems(view, NO_PICKS).filter((problem) => problem.kind === "role")).toEqual([
      { kind: "role", role: 0, have: 0, bounds: { min: 1, max: 1 } },
    ])
    // A role taken back leaves the pick to the remainder.
    expect([...give(split, "c2", { role: null }).roles]).toEqual([["c1", 0]])
  })

  it("wants every pick in a role when no role takes the remainder", () => {
    const view = asked("targets")
    const strict = {
      ...view,
      selection: { ...view.selection, roles: view.selection.roles.slice(0, 2) },
    }
    expect(problems(strict, pick(strict, "c1"))).toContainEqual({
      kind: "unassigned",
      option: "c1",
    })
  })

  it("finds picks that share what has to differ", () => {
    const view = asked("search")
    const [first, second, third] = view.selection.candidates
    if (first === undefined || second === undefined || third === undefined)
      throw new Error("three candidates expected")
    const alike = {
      ...view,
      selection: {
        ...view.selection,
        bounds: { min: 0, max: 3 },
        candidates: [
          { ...first, rules_names: ["A", "B"], original_cost: 1, classes: ["x"] },
          { ...second, rules_names: ["B"], original_cost: 2, classes: ["y"] },
          { ...third, rules_names: ["C"], original_cost: 2, classes: ["y", "x"] },
        ],
      },
    }
    const same = problems(alike, pick(alike, first.id, second.id, third.id)).filter(
      (problem) => problem.kind === "same",
    )
    expect(same).toEqual([
      { kind: "same", property: "rules_name", options: [first.id, second.id] },
      { kind: "same", property: "original_cost", options: [second.id, third.id] },
      { kind: "same", property: "class", options: [first.id, second.id, third.id] },
    ])
    // What the question does not give cannot be compared.
    const unknown = {
      ...alike,
      selection: {
        ...alike.selection,
        candidates: alike.selection.candidates.map((candidate) => ({
          ...candidate,
          rules_names: null,
          original_cost: null,
          classes: null,
        })),
      },
    }
    expect(kinds(unknown, pick(unknown, first.id, second.id))).not.toContain("same")
  })
})

describe("costTotal", () => {
  it("sums the picks' original costs beside the question's limits", () => {
    const view = asked("search")
    const [first, second] = view.selection.candidates
    if (first === undefined || second === undefined) throw new Error("two candidates expected")
    const priced = {
      ...view,
      selection: {
        ...view.selection,
        candidates: [
          { ...first, original_cost: 3 },
          { ...second, original_cost: null },
        ],
      },
    }
    expect(costTotal(priced, pick(priced, first.id))).toEqual({ sum: 3, min: null, max: 4 })
    expect(costTotal(priced, NO_PICKS).sum).toBe(0)
    // A cost that is not given leaves the sum open.
    expect(costTotal(priced, pick(priced, first.id, second.id)).sum).toBeNull()
  })
})

describe("selectAnswer", () => {
  const submit = (command: ReturnType<typeof selectAnswer>) => ({
    kind: "submit",
    request_id: "r1",
    command,
  })

  it("lists every role, the remainder taking what no other role was given", () => {
    const view = asked("targets")
    const picks = give(pick(view, "c2", "c1"), "c2", { role: 0 })
    const command = selectAnswer(view, picks)
    expect(command).toEqual({
      kind: "answer",
      question_id: view.question,
      answer: {
        kind: "select",
        selected: ["c2", "c1"],
        // The answer names no groups: the question only wants them to be satisfiable.
        groups: null,
        roles: [
          { role: 0, selected: ["c2"] },
          { role: 1, selected: [] },
          { role: 2, selected: ["c1"] },
        ],
      },
    })
    expect(validateClientMessage(submit(command))).toBe(true)
  })

  it("names every group, the empty ones too, where the question wants them named", () => {
    const view = asked("search")
    const command = selectAnswer(view, pick(view, "c3"))
    expect(command).toMatchObject({
      answer: {
        selected: ["c3"],
        groups: [
          { group: 0, selected: [] },
          { group: 1, selected: ["c3"] },
        ],
        roles: [],
      },
    })
    expect(validateClientMessage(submit(command))).toBe(true)
  })
})
