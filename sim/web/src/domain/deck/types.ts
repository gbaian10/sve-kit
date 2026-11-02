import type { Region } from "../search"

/** A deck is built on one class (neutral included) or on one title; never both (CR 6.1.1.5). */
export type DeckBasis =
  | { readonly kind: "class"; readonly classCode: string }
  | { readonly kind: "title"; readonly titleCode: string }

/** One card of a deck: the permanent card id, how many, and optionally the printing played. */
export interface DeckEntry {
  readonly cardId: string
  readonly count: number
  readonly printingId?: string
}

export interface DeckLeader {
  readonly cardId: string
  readonly printingId?: string
}

/**
 * A saved deck. `id`, `updatedAt` and `revision` are what a later account sync compares; a deck
 * never changes identity when it is renamed or edited.
 */
export interface Deck {
  readonly id: string
  readonly name: string
  readonly edition: Region
  readonly basis: DeckBasis
  readonly leader: DeckLeader | null
  readonly main: readonly DeckEntry[]
  readonly evolve: readonly DeckEntry[]
  /** Cards under consideration (the maybeboard); never counted toward the deck. */
  readonly side: readonly DeckEntry[]
  readonly createdAt: string
  readonly updatedAt: string
  /** Starts at 1 and grows by one with every change. */
  readonly revision: number
}

export type DeckSection = "main" | "evolve" | "side"

/** What the rules, stats and lists need to know about one card, read from the snapshot. */
export interface DeckCardInfo {
  readonly cardId: string
  /** The front face's printed name in the deck's edition: the "card name" of CR 6.1.1.4. */
  readonly name: string
  readonly classCode: string | null
  readonly typeCode: string | null
  /** Vocabulary `special_kind` codes such as `evolve`, `advance`, `token`. */
  readonly specialKinds: readonly string[]
  /** Title (作品名) codes; empty for cards without one. */
  readonly titles: readonly string[]
  readonly cost: number | null
  /** The snapshot's region `deck_role` when it knows one. */
  readonly deckRole: string | null
}

/** The facts of a Standard restriction a deck check needs. */
export interface DeckRestriction {
  readonly id: string
  readonly maxCopies: number | null
}
