// @vitest-environment node
import { readFileSync } from "node:fs"
import path from "node:path"

import { describe, expect, it } from "vitest"

import { type Command, type Question, validateClientMessage, validateQuestion } from "../contract"
import {
  designated,
  dice,
  diceAnswer,
  driveTrigger,
  driveTriggerAnswer,
  evolutionPayment,
  evolutionPaymentAnswer,
  resourcePayment,
  resourcePaymentAnswer,
} from "./payments"
import { NO_PICKS, problems, toggle } from "./select"

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

function need<T>(value: T | null | undefined): T {
  if (value === null || value === undefined) throw new Error("the sample is another kind")
  return value
}

describe("evolutionPayment", () => {
  it("lists the ways to pay as the question gives them, and pays one of them", () => {
    const question = sample("evolution-payment")
    const asked = need(evolutionPayment(question, question.actor))
    expect(asked.payments).toEqual([
      { pp: 1, use_ep: false, super_evolution: false },
      { pp: 0, use_ep: true, super_evolution: false },
      { pp: 1, use_ep: false, super_evolution: true },
      { pp: 0, use_ep: true, super_evolution: true },
    ])
    const command = evolutionPaymentAnswer(asked, need(asked.payments[3]))
    expect(command).toMatchObject({
      question_id: question.id,
      answer: { kind: "evolution_payment", pp: 0, use_ep: true, super_evolution: true },
    })
    expect(valid(command)).toBe(true)
    expect(evolutionPayment(question, "p2")).toBeNull()
    expect(evolutionPayment(sample("modes"), "p1")).toBeNull()
  })
})

describe("resourcePayment", () => {
  it("reads the resources as a selection, and whether an evolve point may stand in", () => {
    const question = sample("resource-entry-payment")
    const asked = need(resourcePayment(question, question.actor))
    expect(asked.epAllowed).toBe(true)
    expect(asked.materials.selection.bounds).toEqual({ min: 0, max: 2 })
    // The picks are made and counted like any selection's.
    const [first] = asked.materials.selection.candidates
    const picks = toggle(asked.materials, NO_PICKS, need(first).id)
    expect(problems(asked.materials, picks)).toEqual([])
    const command = resourcePaymentAnswer(asked, picks, true)
    expect(command).toMatchObject({
      question_id: question.id,
      answer: { kind: "resource_payment", materials: [need(first).id], use_ep: true },
    })
    expect(valid(command)).toBe(true)
    expect(valid(resourcePaymentAnswer(asked, NO_PICKS, false))).toBe(true)
    expect(resourcePayment(question, "p2")).toBeNull()
  })
})

describe("dice", () => {
  it("reads the roll and keeps it or rolls again", () => {
    const question = sample("dice-reroll")
    const asked = need(dice(question, question.actor))
    expect(asked).toMatchObject({ value: 3, remaining: 1, allowed: ["keep", "reroll"] })
    const command = diceAnswer(asked, "reroll")
    expect(command).toMatchObject({ answer: { kind: "dice_reroll", choice: "reroll" } })
    expect([command, diceAnswer(asked, "keep")].every(valid)).toBe(true)
    expect(dice(question, "p2")).toBeNull()
  })
})

describe("driveTrigger", () => {
  const question = sample("drive-trigger")
  const asked = need(driveTrigger(question, question.actor))

  it("reads the icon, the answers it takes and the followers it may name", () => {
    expect(asked).toMatchObject({
      icon: "critical",
      allowed: [true, false],
      targetBounds: { min: 1, max: 1 },
    })
    expect(asked.targets.map((target) => target.id)).toEqual(["c1", "c3"])
    expect(driveTrigger(question, "p2")).toBeNull()
  })

  it("names one follower when it is accepted, and nobody when it is declined", () => {
    // Accepted, exactly one follower is to be named here.
    expect(designated(asked, "c1")).toBe(true)
    expect(designated(asked, null)).toBe(false)
    // A trigger that names nobody is complete without a follower.
    expect(designated({ ...asked, targetBounds: { min: 0, max: 0 } }, null)).toBe(true)
    const accepted = driveTriggerAnswer(asked, true, "c3")
    expect(accepted).toMatchObject({
      answer: { kind: "drive_trigger", accept: true, target: "c3" },
    })
    const declined = driveTriggerAnswer(asked, false, "c3")
    expect(declined).toMatchObject({ answer: { accept: false, target: null } })
    expect([accepted, declined].every(valid)).toBe(true)
  })
})
