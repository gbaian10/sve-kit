// @vitest-environment node
import { readFileSync } from "node:fs"
import path from "node:path"

import { describe, expect, it } from "vitest"

import { type Command, type Question, validateClientMessage, validateQuestion } from "../contract"
import {
  distributeAnswer,
  distribution,
  inRange,
  integer,
  integerAnswer,
  NO_PILES,
  orderAnswer,
  ordering,
  partition,
  partitionAnswer,
  pileProblems,
  put,
  shareProblems,
  wholeOrder,
} from "./arrange"

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

const valid = (command: Command): boolean =>
  validateClientMessage({ kind: "submit", request_id: "r1", command })

function need<T>(value: T | null): T {
  if (value === null) throw new Error("the sample is another kind of question")
  return value
}

describe("ordering", () => {
  const question = sample("order-cards")
  const asked = need(ordering(question, question.actor))

  it("reads what is to be ordered, how the order is read and where the cards go", () => {
    expect(asked.items.map((item) => item.id)).toEqual(["c1", "c2"])
    expect(asked.direction).toBe("top_down")
    expect(asked.destination).toEqual({ player: "p1", zone: "deck", position: "bottom" })
    expect(ordering(question, "p2")).toBeNull()
    expect(ordering(sample("modes"), "p1")).toBeNull()
    expect(ordering(null, "p1")).toBeNull()
  })

  it("wants every item once, and answers with the order as given", () => {
    expect(wholeOrder(asked, ["c2", "c1"])).toBe(true)
    expect(wholeOrder(asked, ["c2"])).toBe(false)
    expect(wholeOrder(asked, ["c2", "c2"])).toBe(false)
    expect(wholeOrder(asked, ["c2", "c1", "c3"])).toBe(false)
    const command = orderAnswer(asked, ["c2", "c1"])
    expect(command).toMatchObject({ answer: { kind: "order", order: ["c2", "c1"] } })
    expect(valid(command)).toBe(true)
  })
})

describe("partition", () => {
  const question = sample("search-partition")
  const asked = need(partition(question, question.actor))

  it("puts each item on one pile, at the pile's end", () => {
    // One to the hand, two under the deck in an order.
    let piles = put(asked, NO_PILES, "c2", 1)
    piles = put(asked, piles, "c1", 1)
    piles = put(asked, piles, "c3", 0)
    expect([...piles]).toEqual([
      [0, ["c3"]],
      [1, ["c2", "c1"]],
    ])
    // Put on another pile, an item leaves the first; put on its own again, it goes to the end.
    expect([...put(asked, piles, "c1", 0)]).toEqual([
      [0, ["c3", "c1"]],
      [1, ["c2"]],
    ])
    expect(put(asked, piles, "c2", 1).get(1)).toEqual(["c1", "c2"])
    // Taken off every pile, and a pile left without items is gone.
    expect([...put(asked, piles, "c3", null)]).toEqual([[1, ["c2", "c1"]]])
    // What the question does not list is not taken.
    expect(put(asked, piles, "nothing", 0)).toBe(piles)
    expect(put(asked, piles, "c1", 7)).toBe(piles)
  })

  it("says which items lie nowhere and which piles hold too few or too many", () => {
    expect(pileProblems(asked, NO_PILES)).toEqual([
      { kind: "unplaced", option: "c1" },
      { kind: "unplaced", option: "c2" },
      { kind: "unplaced", option: "c3" },
      { kind: "pile", bucket: 0, have: 0, bounds: { min: 1, max: 1 } },
      { kind: "pile", bucket: 1, have: 0, bounds: { min: 2, max: 2 } },
    ])
    const hand = ["c1", "c2"].reduce((piles, item) => put(asked, piles, item, 0), NO_PILES)
    expect(pileProblems(asked, put(asked, hand, "c3", 1))).toEqual([
      { kind: "pile", bucket: 0, have: 2, bounds: { min: 1, max: 1 } },
      { kind: "pile", bucket: 1, have: 1, bounds: { min: 2, max: 2 } },
    ])
    expect(pileProblems(asked, put(asked, put(asked, hand, "c3", 1), "c2", 1))).toEqual([])
  })

  it("answers with every pile, the empty ones too, each in its order", () => {
    const piles = put(asked, put(asked, NO_PILES, "c3", 1), "c1", 1)
    const command = partitionAnswer(asked, piles)
    expect(command).toMatchObject({
      answer: {
        kind: "partition",
        buckets: [
          { bucket: 0, items: [] },
          { bucket: 1, items: ["c3", "c1"] },
        ],
      },
    })
    expect(valid(command)).toBe(true)
    expect(partition(question, "p2")).toBeNull()
  })
})

describe("distribution", () => {
  const question = sample("damage-distribution")
  const asked = need(distribution(question, question.actor))

  it("holds the amounts against the total and against what one recipient may get", () => {
    expect(asked).toMatchObject({ total: 3, share: { min: 0, max: null } })
    expect(shareProblems(asked, new Map())).toEqual([{ kind: "total", have: 0, want: 3 }])
    expect(shareProblems(asked, new Map([["c1", 3]]))).toEqual([])
    expect(
      shareProblems(
        asked,
        new Map([
          ["c1", 2],
          ["c2", 2],
        ]),
      ),
    ).toEqual([{ kind: "total", have: 4, want: 3 }])
    const capped = { ...asked, share: { min: 1, max: 2 } }
    expect(shareProblems(capped, new Map([["c1", 3]]))).toEqual([
      { kind: "share", option: "c1", have: 3, bounds: { min: 1, max: 2 } },
      { kind: "share", option: "c2", have: 0, bounds: { min: 1, max: 2 } },
    ])
  })

  it("takes the empty answer with nobody to deal to, or no answer at all, as the question says", () => {
    const nobody = { ...asked, recipients: [] }
    // The total goes undealt where the question allows it.
    expect(shareProblems(nobody, new Map())).toEqual([])
    // Where it does not, nothing is an answer, whatever the total: not even a total of nothing.
    for (const total of [3, 0])
      expect(shareProblems({ ...nobody, total, empty: "reject" }, new Map())).toEqual([
        { kind: "nobody" },
      ])
    // With somebody to deal to, a total of nothing is reached by dealing nothing.
    expect(shareProblems({ ...asked, total: 0, empty: "reject" }, new Map())).toEqual([])
  })

  it("takes only amounts that are counts", () => {
    expect(
      shareProblems(
        asked,
        new Map([
          ["c1", 1.5],
          ["c2", 1.5],
        ]),
      ),
    ).toEqual([
      { kind: "share", option: "c1", have: 1.5, bounds: { min: 0, max: null } },
      { kind: "share", option: "c2", have: 1.5, bounds: { min: 0, max: null } },
    ])
  })

  it("answers with every recipient once, those given nothing included", () => {
    const command = distributeAnswer(asked, new Map([["c2", 3]]))
    expect(command).toMatchObject({
      answer: {
        kind: "distribute",
        shares: [
          { option: "c1", amount: 0 },
          { option: "c2", amount: 3 },
        ],
      },
    })
    expect(valid(command)).toBe(true)
    expect(distribution(question, "p2")).toBeNull()
  })
})

describe("integer", () => {
  const question = sample("declare-number")
  const asked = need(integer(question, question.actor))

  it("takes a whole number within the question's range", () => {
    expect(asked.bounds).toEqual({ min: 2, max: 5 })
    expect([1, 2, 5, 6, 2.5].map((value) => inRange(asked, value))).toEqual([
      false,
      true,
      true,
      false,
      false,
    ])
    expect(inRange({ ...asked, bounds: { min: 0, max: null } }, 99)).toBe(true)
    const command = integerAnswer(asked, 4)
    expect(command).toMatchObject({ answer: { kind: "integer", value: 4 } })
    expect(valid(command)).toBe(true)
    expect(integer(question, "p2")).toBeNull()
  })

  it("stops a range without an upper end at the largest number the contract carries", () => {
    const open = { ...asked, bounds: { min: 0, max: null } }
    const largest = Number.MAX_SAFE_INTEGER
    expect(inRange(open, largest)).toBe(true)
    expect(valid(integerAnswer(open, largest))).toBe(true)
    // One more is a whole number to JavaScript and no message of the contract.
    expect(Number.isInteger(largest + 1)).toBe(true)
    expect(inRange(open, largest + 1)).toBe(false)
    expect(valid(integerAnswer(open, largest + 1))).toBe(false)
  })
})
