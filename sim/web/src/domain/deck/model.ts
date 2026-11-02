import type { Region } from "../search"
import type { Deck, DeckBasis, DeckEntry } from "./types"

export function totalCount(entries: readonly DeckEntry[]): number {
  return entries.reduce((sum, entry) => sum + entry.count, 0)
}

export function createDeck(
  input: { readonly name: string; readonly edition: Region; readonly basis: DeckBasis },
  now: Date,
): Deck {
  const stamp = now.toISOString()
  return {
    id: crypto.randomUUID(),
    name: input.name,
    edition: input.edition,
    basis: input.basis,
    leader: null,
    main: [],
    evolve: [],
    side: [],
    createdAt: stamp,
    updatedAt: stamp,
    revision: 1,
  }
}

/** A new deck with the same cards: its own identity and history, nothing shared with the source. */
export function duplicateDeck(deck: Deck, name: string, now: Date): Deck {
  const stamp = now.toISOString()
  return { ...deck, id: crypto.randomUUID(), name, createdAt: stamp, updatedAt: stamp, revision: 1 }
}

export function renameDeck(deck: Deck, name: string, now: Date): Deck {
  return { ...deck, name, updatedAt: now.toISOString(), revision: deck.revision + 1 }
}
