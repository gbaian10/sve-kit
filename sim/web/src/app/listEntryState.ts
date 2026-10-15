import { useCallback } from "react"
import { useLocation, useNavigate } from "react-router"

import type { CardSummary } from "../data"

export const LIST_PAGE_SIZE = 60

/** What the list keeps on its own history entry so "back" can rebuild it (architecture §2.2). */
export interface ListEntryState {
  readonly pages?: number
  readonly anchor?: string
}

/** What a card entry receives from the list or suggest list that opened it. */
export interface CardEntryState {
  readonly route?: {
    readonly cardId: string
    readonly printingId: string
    readonly cardOwner: string
    readonly printingOwner: string
    readonly region: "jp" | "en"
  }
  readonly background: string
  readonly source: "results" | "suggest"
  readonly pages: number
  readonly resultKey: string
}

/** In-site links carry the printing and both owners, so the card page skips the cold lookup. */
export function routeState(summary: CardSummary): Pick<CardEntryState, "route"> {
  return summary.route
    ? {
        route: {
          ...summary.route,
          cardId: summary.cardId,
          printingId: summary.printingId,
          region: summary.region,
        },
      }
    : {}
}

function isListEntryState(value: unknown): value is ListEntryState {
  return typeof value === "object" && value !== null
}

/**
 * Reads and updates the list's own entry state through the router only, never through
 * `history.replaceState`, which would drop the fields react-router keeps in `history.state`.
 */
export function useListEntryState(): readonly [ListEntryState, (patch: ListEntryState) => void] {
  const location = useLocation()
  const navigate = useNavigate()
  const state: ListEntryState = isListEntryState(location.state) ? location.state : {}
  const update = useCallback(
    (patch: ListEntryState) => {
      void navigate(location.pathname + location.search, {
        replace: true,
        state: { ...(isListEntryState(location.state) ? location.state : {}), ...patch },
      })
    },
    [navigate, location.pathname, location.search, location.state],
  )
  return [state, update]
}
