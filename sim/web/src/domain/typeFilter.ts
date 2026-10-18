// The type row's chip order (the maintainer's). Token, leader, EP and SEP moved to the deck
// location row (`deckLocation.ts`); a type here is the printed type whatever the deck location.
export const TYPE_ORDER = ["follower", "spell", "amulet"] as const

/** Selected type codes in the row's display order; codes the order does not know follow it. */
export function orderTypeTokens(tokens: readonly string[]): string[] {
  const rank = (code: string) => {
    const index = (TYPE_ORDER as readonly string[]).indexOf(code)
    return index === -1 ? TYPE_ORDER.length : index
  }
  return [...tokens].sort((a, b) => rank(a) - rank(b))
}
