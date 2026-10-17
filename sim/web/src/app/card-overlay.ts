import type { CardRouteInput } from "../data"
import type { CardOpenMode } from "../settings"
import { usePrefs } from "../settings"
import { useMediaQuery } from "./useMediaQuery"

/** The list's URL names the card it shows beside or over the results: `?card=PR-002`. */
export const CARD_PARAM = "card"
const PROVISIONAL_PREFIX = "_provisional/"

/** Same keys as the card routes, so the panel and the page resolve one card the same way. */
export function cardParamOf(printing: {
  readonly cardNo: string
  readonly cardNoState?: string
  readonly intId?: number
}): string {
  return printing.cardNoState === "provisional" && printing.intId !== undefined
    ? `${PROVISIONAL_PREFIX}${String(printing.intId)}`
    : printing.cardNo
}

export function parseCardParam(
  value: string | null,
): Pick<CardRouteInput, "namespace" | "key"> | null {
  if (value === null || value === "") return null
  return value.startsWith(PROVISIONAL_PREFIX)
    ? { namespace: "provisional", key: value.slice(PROVISIONAL_PREFIX.length) }
    : { namespace: "official", key: value }
}

export function withoutCardParam(search: string): string {
  const params = new URLSearchParams(search)
  params.delete(CARD_PARAM)
  const rest = params.toString()
  return rest === "" ? "" : `?${rest}`
}

export type CardPresentation = Exclude<CardOpenMode, "auto">

// The side panel needs the desktop layout (the `lg` breakpoint); narrower screens get the modal
// even when the preference asks for the panel.
const DESKTOP_QUERY = "(min-width: 1000px)"

function resolvePresentation(mode: CardOpenMode, desktop: boolean): CardPresentation {
  if (mode === "page" || mode === "modal") return mode
  return desktop ? "panel" : "modal"
}

export function useCardPresentation(): CardPresentation {
  const { cardOpenMode } = usePrefs()
  return resolvePresentation(cardOpenMode, useMediaQuery(DESKTOP_QUERY))
}
