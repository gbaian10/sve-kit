// @vitest-environment node
import { describe, expect, it } from "vitest"

import { type Command, type Question, validateClientMessage } from "../contract"
import { SCENES } from "../fixtures/scenes"
import {
  choice,
  chooseAnswer,
  chosenEnough,
  confirmAnswer,
  confirmation,
  mulligan,
  mulliganAnswer,
  wholeHand,
} from "./choices"

/** The question a fixture scene opens with for its first seat. */
function opening(id: string): Question {
  const first = SCENES.find((scene) => scene.id === id)?.script.p1.find(
    (message) => message.kind === "update",
  )
  if (first?.input.kind !== "question") throw new Error(`${id} asks nothing`)
  return first.input.question
}

const valid = (command: Command): boolean =>
  validateClientMessage({ kind: "submit", request_id: "r1", command })

describe("choice", () => {
  it("reads the options of a choice put to this seat", () => {
    const question = opening("modes")
    const asked = choice(question, "p1")
    expect(asked).toMatchObject({ question: "modes", bounds: { min: 1, max: 1 }, count: 1 })
    expect(asked?.options.map((option) => option.value)).toEqual([
      { kind: "mode", mode: 1, text: null },
      { kind: "mode", mode: 2, text: null },
    ])
    expect(choice(question, "p2")).toBeNull()
    expect(choice(opening("targets"), "p1")).toBeNull()
    expect(choice(null, "p1")).toBeNull()
  })

  it("counts the chosen options, and answers in the question's own order", () => {
    const asked = choice(opening("modes"), "p1")
    if (asked === null) throw new Error("a choice expected")
    expect(chosenEnough(asked, [])).toBe(false)
    expect(chosenEnough(asked, ["mode:2"])).toBe(true)
    expect(chosenEnough(asked, ["mode:2", "mode:1"])).toBe(false)
    const wide = { ...asked, bounds: { min: 0, max: null } }
    expect(chosenEnough(wide, [])).toBe(true)
    expect(chosenEnough(wide, ["mode:2", "mode:1"])).toBe(true)
    const command = chooseAnswer(wide, ["mode:2", "mode:1"])
    expect(command).toMatchObject({ answer: { kind: "choose", chosen: ["mode:1", "mode:2"] } })
    expect(valid(command)).toBe(true)
  })
})

describe("mulligan", () => {
  it("keeps the hand, or returns the whole of it in an order", () => {
    const question = opening("mulligan")
    const asked = mulligan(question, "p1")
    if (asked === null) throw new Error("the opening hand expected")
    expect(asked.hand).toHaveLength(4)
    const ids = asked.hand.map((card) => card.id)
    const keep = mulliganAnswer(asked, null)
    expect(keep).toMatchObject({ answer: { kind: "mulligan", redraw: false, bottom_order: [] } })
    const order = [...ids].reverse()
    const redraw = mulliganAnswer(asked, order)
    expect(redraw).toMatchObject({ answer: { redraw: true, bottom_order: order } })
    expect([keep, redraw].every(valid)).toBe(true)
    // Every card once: no part of the hand, no card twice.
    expect(wholeHand(asked, order)).toBe(true)
    expect(wholeHand(asked, ids.slice(1))).toBe(false)
    expect(wholeHand(asked, [...ids.slice(1), ids[1] ?? ""])).toBe(false)
    expect(mulligan(question, "p2")).toBeNull()
    expect(mulligan(opening("modes"), "p1")).toBeNull()
  })
})

describe("confirmation", () => {
  it("reads what the yes or no is about and which answers it takes", () => {
    const question = opening("sisters")
    const asked = confirmation(question, "p1")
    expect(asked).toMatchObject({
      question: "extra-cost",
      allowed: [true, false],
      subject: { kind: "card", object: "a:sisters" },
      origin: { player: "p1", zone: "hand", position: 0 },
    })
    if (asked === null) throw new Error("a yes or no expected")
    const command = confirmAnswer(asked, true)
    expect(command).toMatchObject({ answer: { kind: "confirm", accept: true } })
    expect(valid(command)).toBe(true)
    expect(confirmation(question, "p2")).toBeNull()
    expect(confirmation(opening("modes"), "p1")).toBeNull()
  })
})
