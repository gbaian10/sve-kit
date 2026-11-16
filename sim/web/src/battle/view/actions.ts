// What the open question lets a seat do in its main phase, read straight from the question. Like
// the table model, nothing here works out a rule: which cards may be played, what may attack and
// what each attacker may attack are exactly the intents the authority listed.
import type { Command, ObjectHandle, OptionId, Question, QuestionId, Seat } from "../contract"

interface PlayAction {
  readonly option: OptionId
  /** Play points the authority quotes for this play; null without a settled quote. */
  readonly cost: number | null
}

interface AttackAction {
  readonly option: OptionId
  /** The option that attacks the opposing leader; null when the leader is no legal recipient. */
  readonly leader: OptionId | null
  /** The cards that may be attacked, each with the option that names it. */
  readonly cards: ReadonlyMap<ObjectHandle, OptionId>
}

export interface Actions {
  readonly question: QuestionId
  /** The ways each card may be played, by the card. */
  readonly play: ReadonlyMap<ObjectHandle, readonly PlayAction[]>
  /** Abilities that may be activated and evolutions that may be made, by the card that holds them. */
  readonly activate: ReadonlyMap<ObjectHandle, readonly OptionId[]>
  readonly evolve: ReadonlyMap<ObjectHandle, readonly OptionId[]>
  /** Attackers, each with its own legal recipients. */
  readonly attack: ReadonlyMap<ObjectHandle, AttackAction>
  /** Ending the phase; null when the question does not offer it. */
  readonly endPhase: OptionId | null
}

/**
 * The actions of a main-phase question put to `viewer`. Null when nothing is asked, when another
 * seat is asked, and for any other kind of question: those are not answered from the table.
 */
export function actions(question: Question | null, viewer: Seat): Actions | null {
  if (question?.actor !== viewer || question.shape.kind !== "action_window") return null
  const play = new Map<ObjectHandle, PlayAction[]>()
  const activate = new Map<ObjectHandle, OptionId[]>()
  const evolve = new Map<ObjectHandle, OptionId[]>()
  const attack = new Map<ObjectHandle, AttackAction>()
  let endPhase: OptionId | null = null
  for (const intent of question.shape.intents) {
    if (intent.kind === "play")
      play.set(intent.source, [
        ...(play.get(intent.source) ?? []),
        { option: intent.id, cost: intent.cost?.total_pp ?? null },
      ])
    else if (intent.kind === "activate")
      activate.set(intent.holder, [...(activate.get(intent.holder) ?? []), intent.id])
    else if (intent.kind === "evolve")
      evolve.set(intent.holder, [...(evolve.get(intent.holder) ?? []), intent.id])
    else if (intent.kind === "attack") {
      const cards = new Map<ObjectHandle, OptionId>()
      let leader: OptionId | null = null
      for (const option of intent.targets) {
        if (option.target.kind === "leader") leader = option.id
        else cards.set(option.target.object, option.id)
      }
      attack.set(intent.attacker, { option: intent.id, leader, cards })
    } else endPhase = intent.id
  }
  return { question: question.id, play, activate, evolve, attack, endPhase }
}

/** The command that takes one of the question's intents; an attack names its recipient too. */
export function act(acts: Actions, intent: OptionId, target: OptionId | null = null): Command {
  return {
    kind: "answer",
    question_id: acts.question,
    answer: { kind: "action_window", intent, target },
  }
}
