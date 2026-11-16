// The questions that pay for something or answer a roll or a trigger, as the screen works with
// them. Like the other readers they repeat what the question says and build its answer; which
// payments are legal is the question's own list, never worked out from the seat's points.
import type {
  Bounds,
  Candidate,
  Command,
  DiceChoice,
  EvolutionPayment,
  OptionId,
  Question,
  QuestionId,
  Seat,
  TriggerIcon,
} from "../contract"
import type { Picks, Select } from "./select"

export interface EvolutionPay {
  readonly question: QuestionId
  /** Every way the evolution may be paid for. */
  readonly payments: readonly EvolutionPayment[]
}

/** The evolution payment put to `viewer`; null for anything else. */
export function evolutionPayment(question: Question | null, viewer: Seat): EvolutionPay | null {
  if (question?.actor !== viewer || question.shape.kind !== "evolution_payment") return null
  return { question: question.id, payments: question.shape.payments }
}

/** The command that pays one of the ways the question lists. */
export function evolutionPaymentAnswer(asked: EvolutionPay, payment: EvolutionPayment): Command {
  return {
    kind: "answer",
    question_id: asked.question,
    answer: { kind: "evolution_payment", ...payment },
  }
}

export interface ResourcePay {
  /** The resources to pay with, as a selection like any other. */
  readonly materials: Select
  /** Whether one evolve point may stand in for one play point. */
  readonly epAllowed: boolean
}

/** The resource payment put to `viewer`; null for anything else. */
export function resourcePayment(question: Question | null, viewer: Seat): ResourcePay | null {
  if (question?.actor !== viewer || question.shape.kind !== "resource_payment") return null
  return {
    materials: { question: question.id, selection: question.shape.materials },
    epAllowed: question.shape.ep_allowed,
  }
}

/** The command that pays with the picked resources, an evolve point for a play point or not. */
export function resourcePaymentAnswer(asked: ResourcePay, picks: Picks, useEp: boolean): Command {
  return {
    kind: "answer",
    question_id: asked.materials.question,
    answer: { kind: "resource_payment", materials: [...picks.selected], use_ep: useEp },
  }
}

export interface Dice {
  readonly question: QuestionId
  /** What the die shows. */
  readonly value: number
  /** How many rerolls are left. */
  readonly remaining: number
  /** The answers the question takes. */
  readonly allowed: readonly DiceChoice[]
}

/** The rolled die put to `viewer`; null for anything else. */
export function dice(question: Question | null, viewer: Seat): Dice | null {
  if (question?.actor !== viewer || question.shape.kind !== "dice_reroll") return null
  const { value, remaining, allowed } = question.shape
  return { question: question.id, value, remaining, allowed }
}

export function diceAnswer(asked: Dice, choice: DiceChoice): Command {
  return { kind: "answer", question_id: asked.question, answer: { kind: "dice_reroll", choice } }
}

export interface DriveTrigger {
  readonly question: QuestionId
  readonly icon: TriggerIcon
  /** Whether the trigger may be accepted, declined, or both. */
  readonly allowed: readonly boolean[]
  /** The followers the icon may designate. */
  readonly targets: readonly Candidate[]
  /** How many of them an accepted trigger designates; the answer names one at most. */
  readonly targetBounds: Bounds
}

/** The drive trigger put to `viewer`; null for anything else. */
export function driveTrigger(question: Question | null, viewer: Seat): DriveTrigger | null {
  if (question?.actor !== viewer || question.shape.kind !== "drive_trigger") return null
  const { icon, allowed, targets, target_bounds: targetBounds } = question.shape
  return { question: question.id, icon, allowed, targets, targetBounds }
}

/**
 * Whether an accepted trigger has as many followers designated as the question takes. Declining
 * designates nobody.
 */
export function designated(asked: DriveTrigger, target: OptionId | null): boolean {
  const have = target === null ? 0 : 1
  const { min, max } = asked.targetBounds
  return have >= min && (max === null || have <= max)
}

/** The command that accepts the trigger, with the follower it designates, or declines it. */
export function driveTriggerAnswer(
  asked: DriveTrigger,
  accept: boolean,
  target: OptionId | null,
): Command {
  return {
    kind: "answer",
    question_id: asked.question,
    answer: { kind: "drive_trigger", accept, target: accept ? target : null },
  }
}
