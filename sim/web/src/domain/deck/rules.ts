import {
  checkConstruction,
  type ConstructionIndex,
  MAIN_MAX,
  type NoteCode,
  type ViolationCode,
} from "./construction"
import type { Deck } from "./types"

type ProblemSeverity = "error" | "info"

export interface DeckProblem {
  readonly kind: ViolationCode | NoteCode
  /** Errors make the deck illegal for Standard; infos are checks that could not be made. */
  readonly severity: ProblemSeverity
  /** Comprehensive Rules clause, `restriction` for the official lists, `format` for formats. */
  readonly clause: string
  /** Cards the message is about; the deck list scrolls to the first. */
  readonly cardIds: readonly string[]
  readonly values: Readonly<Record<string, string | number>>
}

/** The deck's Standard problems on a day, errors first, with card names filled in for messages. */
export function deckProblems(
  deck: Deck,
  index: ConstructionIndex,
  date: string,
  nameOf: (cardId: string) => string | undefined,
): DeckProblem[] {
  const result = checkConstruction(
    {
      basis:
        deck.basis.kind === "class"
          ? { kind: "class", classCode: deck.basis.classCode }
          : { kind: "title", titleCode: deck.basis.titleCode },
      leader: deck.leader?.cardId ?? null,
      main: deck.main.map((entry) => [entry.cardId, entry.count] as const),
      evolve: deck.evolve.map((entry) => [entry.cardId, entry.count] as const),
    },
    index,
    { format: deck.format, date },
  )
  // The panel's messages also show the main deck's range and the first card's printed name.
  const named = (cardIds: readonly string[], values: Readonly<Record<string, string | number>>) => {
    const first = cardIds[0]
    if (first === undefined)
      return values["min"] === undefined ? values : { ...values, max: MAIN_MAX }
    // `name` arrives as a rules name code; the message shows the first card's printed name.
    return { ...values, name: nameOf(first) ?? first, cardId: first }
  }
  return [
    ...result.violations.map((violation): DeckProblem => ({
      kind: violation.code,
      severity: "error",
      clause: violation.clause,
      cardIds: violation.cards,
      values: named(violation.cards, violation.values),
    })),
    ...result.notes.map((note): DeckProblem => ({
      kind: note.code,
      severity: "info",
      clause: note.clause,
      cardIds: [],
      values: {},
    })),
  ]
}
