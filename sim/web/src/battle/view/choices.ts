// The three small questions as the screen works with them: choosing among typed options, keeping
// or redrawing the opening hand, and a yes or no. Like the other readers they repeat what the
// question says and build its answer; they work out no rule.
import type {
  Bounds,
  Candidate,
  Capacity,
  ChoiceOption,
  Command,
  EarlierChoice,
  OptionId,
  Origin,
  Question,
  QuestionId,
  Seat,
  Subject,
} from "../contract"

export interface Choose {
  readonly question: QuestionId
  /** The options in their meaningful order. */
  readonly options: readonly ChoiceOption[]
  readonly bounds: Bounds
  /** How many selections a mode choice comes to, when the question says. */
  readonly count: number | null
  readonly capacity: Capacity | null
  /** Earlier choices of the same batch, shown for context; they are not answered. */
  readonly earlier: readonly EarlierChoice[]
}

/** The choice put to `viewer`; null for anything else. */
export function choice(question: Question | null, viewer: Seat): Choose | null {
  if (question?.actor !== viewer || question.shape.kind !== "choose") return null
  const { options, bounds, count, capacity, earlier } = question.shape
  return { question: question.id, options, bounds, count, capacity, earlier }
}

/** Whether as many options are chosen as the question takes; it says nothing more about them. */
export function chosenEnough(asked: Choose, chosen: readonly OptionId[]): boolean {
  const { min, max } = asked.bounds
  return chosen.length >= min && (max === null || chosen.length <= max)
}

/**
 * The command that answers with the chosen options. They are sent in the question's own order:
 * the order of an answer means nothing, the printed order decides.
 */
export function chooseAnswer(asked: Choose, chosen: readonly OptionId[]): Command {
  return {
    kind: "answer",
    question_id: asked.question,
    answer: {
      kind: "choose",
      chosen: asked.options.flatMap(({ id }) => (chosen.includes(id) ? [id] : [])),
    },
  }
}

export interface Mulligan {
  readonly question: QuestionId
  /** The whole opening hand. */
  readonly hand: readonly Candidate[]
}

/** The opening-hand question put to `viewer`; null for anything else. */
export function mulligan(question: Question | null, viewer: Seat): Mulligan | null {
  if (question?.actor !== viewer || question.shape.kind !== "mulligan") return null
  return { question: question.id, hand: question.shape.hand }
}

/** Whether an order names every card of the hand exactly once, as a redraw has to. */
export function wholeHand(asked: Mulligan, order: readonly OptionId[]): boolean {
  return (
    order.length === asked.hand.length &&
    asked.hand.every((card) => order.filter((id) => id === card.id).length === 1)
  )
}

/**
 * The command that keeps the hand, or returns it in `order`, top to bottom, and draws again.
 * There is no exchanging part of a hand.
 */
export function mulliganAnswer(asked: Mulligan, order: readonly OptionId[] | null): Command {
  return {
    kind: "answer",
    question_id: asked.question,
    answer: { kind: "mulligan", redraw: order !== null, bottom_order: [...(order ?? [])] },
  }
}

export interface Confirm {
  readonly question: QuestionId
  /** The answers the question takes: both, or only one of them. */
  readonly allowed: readonly boolean[]
  /** The card the decision is about, when it concerns one, and where that card is. */
  readonly subject: Subject | null
  readonly origin: Origin | null
}

/** The yes or no put to `viewer`; null for anything else. */
export function confirmation(question: Question | null, viewer: Seat): Confirm | null {
  if (question?.actor !== viewer || question.shape.kind !== "confirm") return null
  const { allowed, subject, origin } = question.shape
  return { question: question.id, allowed, subject, origin }
}

export function confirmAnswer(asked: Confirm, accept: boolean): Command {
  return { kind: "answer", question_id: asked.question, answer: { kind: "confirm", accept } }
}
