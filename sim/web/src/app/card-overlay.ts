import type { CardRouteInput } from "../data"
import type { CardOpenMode } from "../settings"
import { usePrefs } from "../settings"
import { useMediaQuery } from "./useMediaQuery"

/** The list's URL names the card it shows beside or over the results: `?card=PR-002`. */
export const CARD_PARAM = "card"
const PROVISIONAL_PREFIX = "_provisional/"

/** Same keys as the card routes, so the drawer and the page resolve one card the same way. */
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

// The drawer pushes the whole app frame left, so it needs room beside it; narrower screens get the
// modal even when the preference asks for the drawer.
const DRAWER_QUERY = "(min-width: 900px)"

function resolvePresentation(mode: CardOpenMode, wide: boolean): CardPresentation {
  if (mode === "page" || mode === "modal") return mode
  return wide ? "panel" : "modal"
}

export function useCardPresentation(): CardPresentation {
  const { cardOpenMode } = usePrefs()
  return resolvePresentation(cardOpenMode, useMediaQuery(DRAWER_QUERY))
}

/** Set on the root element while the drawer is open; the app frame pads its right side by it. */
export const DRAWER_WIDTH_VAR = "--drawer-width"
export const DRAWER_MIN_PX = 600
const DRAWER_MAX_PX = 1200
// What the results keep at least when the drawer is dragged wide.
const RESULTS_MIN_PX = 480

export function drawerWidthValue(preferred: number | null): string {
  return preferred === null ? "clamp(600px, 34vw, 720px)" : `${String(preferred)}px`
}

export function clampDrawerWidth(px: number, viewportWidth: number): number {
  const max = Math.max(DRAWER_MIN_PX, Math.min(DRAWER_MAX_PX, viewportWidth - RESULTS_MIN_PX))
  return Math.min(max, Math.max(DRAWER_MIN_PX, Math.round(px)))
}
