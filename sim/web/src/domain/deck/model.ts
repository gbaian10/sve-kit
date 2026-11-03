import type { Region } from "../search"
import type { Deck, DeckBasis, DeckCardInfo, DeckEntry } from "./types"

export type Placement = "main" | "evolve" | "leader" | "none"

/** Where a card goes (6.1.1.2, 6.1.1.3): leaders lead, tokens and unknown kinds go nowhere. */
export function placementOf(
  info: Pick<DeckCardInfo, "typeCode" | "specialKinds" | "deckRole">,
): Placement {
  if (info.typeCode === "leader") return "leader"
  if (info.specialKinds.includes("token")) return "none"
  if (info.specialKinds.includes("evolve") || info.specialKinds.includes("advance")) return "evolve"
  if (info.deckRole === "evolve") return "evolve"
  if (
    info.typeCode === "follower" ||
    info.typeCode === "spell" ||
    info.typeCode === "amulet" ||
    info.deckRole === "main"
  )
    return "main"
  return "none"
}

export function countOf(entries: readonly DeckEntry[], cardId: string): number {
  return entries.find((entry) => entry.cardId === cardId)?.count ?? 0
}

/**
 * The section with one card at `count` (0 removes it); a new card goes last. A printing given
 * here replaces the entry's chosen printing, none keeps it.
 */
export function withCount(
  entries: readonly DeckEntry[],
  cardId: string,
  count: number,
  printingId?: string,
): DeckEntry[] {
  if (count <= 0) return entries.filter((entry) => entry.cardId !== cardId)
  const chosen = printingId === undefined ? {} : { printingId }
  if (!entries.some((entry) => entry.cardId === cardId))
    return [...entries, { cardId, count, ...chosen }]
  return entries.map((entry) => (entry.cardId === cardId ? { ...entry, count, ...chosen } : entry))
}

/** The section with the card's printing set, or back to the edition's default (undefined). */
export function withPrinting(
  entries: readonly DeckEntry[],
  cardId: string,
  printingId: string | undefined,
): DeckEntry[] {
  return entries.map((entry) => {
    if (entry.cardId !== cardId) return entry
    const { printingId: _previous, ...rest } = entry
    return printingId === undefined ? rest : { ...rest, printingId }
  })
}

/** Any change to a deck's content: the same deck, a newer time and the next revision. */
export function editDeck(
  deck: Deck,
  change: Partial<Omit<Deck, "id" | "createdAt">>,
  now: Date,
): Deck {
  return { ...deck, ...change, updatedAt: now.toISOString(), revision: deck.revision + 1 }
}

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
  return editDeck(deck, { name }, now)
}
