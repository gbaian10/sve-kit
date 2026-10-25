import { createContext, type MouseEvent, useContext } from "react"
import { type To, useNavigate } from "react-router"

import type { CardSummary } from "../../data"
import { printingPath } from "../../domain/cardRoute"

/** Where a link to a card goes, and the history state it carries. */
export interface CardLinkTarget {
  readonly to: To
  readonly state?: unknown
  /** The target is the printing the shown one was reached from: following it goes back one entry. */
  readonly back?: boolean
}

/**
 * Click handler for a card link: a link back to the previous entry's printing performs the
 * browser's back (restoring that entry's scroll position) instead of stacking another entry.
 */
export function useCardLinkClick(): (
  link: Pick<CardLinkTarget, "back">,
) => (event: MouseEvent<HTMLAnchorElement>) => void {
  const navigate = useNavigate()
  return (link) => (event) => {
    if (
      !link.back ||
      event.defaultPrevented ||
      event.button !== 0 ||
      event.metaKey ||
      event.ctrlKey ||
      event.shiftKey ||
      event.altKey
    )
      return
    event.preventDefault()
    void navigate(-1)
  }
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
