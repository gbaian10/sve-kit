import { createContext, useContext } from "react"
import type { To } from "react-router"

import type { CardSummary } from "../../data"
import { printingPath } from "../../domain/cardRoute"

/** Where a link to a card goes, and the history state it carries. */
export interface CardLinkTarget {
  readonly to: To
  readonly state?: unknown
}

/**
 * How card-name references in card text reach their card. The list's card drawer provides this so a
 * reference opens in the drawer like the related thumbnails do; elsewhere the card's own page opens.
 */
export const CardLinkContext = createContext<((card: CardSummary) => CardLinkTarget) | null>(null)

export function useCardLinkTarget(): (card: CardSummary) => CardLinkTarget {
  const linkOf = useContext(CardLinkContext)
  return linkOf ?? defaultLink
}

function defaultLink(card: CardSummary): CardLinkTarget {
  return { to: printingPath(card) }
}
