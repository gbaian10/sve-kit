import type { DeckCardInfo } from "../domain/deck/types"

/** A small synthetic card set for the deck rules and stats tests. */
export function card(cardId: string, overrides: Partial<DeckCardInfo> = {}): DeckCardInfo {
  return {
    cardId,
    name: cardId,
    classCode: "forest",
    typeCode: "follower",
    specialKinds: [],
    titles: [],
    cost: 2,
    deckRole: null,
    ...overrides,
  }
}

export function lookup(...cards: DeckCardInfo[]): (cardId: string) => DeckCardInfo | undefined {
  const byId = new Map(cards.map((item) => [item.cardId, item]))
  return (cardId) => byId.get(cardId)
}
