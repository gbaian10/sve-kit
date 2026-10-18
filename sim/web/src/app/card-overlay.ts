import { useSyncExternalStore } from "react"

import type { CardRouteInput } from "../data"
import type { CardOpenMode } from "../settings"
import { usePrefs } from "../settings"

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

/** Set on the root element while the drawer is open; the app frame pads its right side by it. */
export const DRAWER_WIDTH_VAR = "--drawer-width"
export const DRAWER_MIN_PX = 600
const DRAWER_MAX_PX = 1200
// The default takes about a third of the window, within the band the maintainer asked for.
const DRAWER_SHARE = 0.34
const DRAWER_DEFAULT_MAX_PX = 720
/** What the results keep beside the drawer; below that the card opens as a modal instead. */
const RESULTS_MIN_PX = 400

/** Whether a drawer of the minimum width still leaves usable results beside it. */
export function drawerFits(viewportWidth: number): boolean {
  return viewportWidth >= DRAWER_MIN_PX + RESULTS_MIN_PX
}

export function clampDrawerWidth(px: number, viewportWidth: number): number {
  const max = Math.max(DRAWER_MIN_PX, Math.min(DRAWER_MAX_PX, viewportWidth - RESULTS_MIN_PX))
  return Math.min(max, Math.max(DRAWER_MIN_PX, Math.round(px)))
}

/**
 * The width the drawer and the app frame both use right now: the saved preference (or the
 * default share of the window), kept inside the current viewport so the results stay beside it.
 */
export function effectiveDrawerWidth(preferred: number | null, viewportWidth: number): number {
  const wanted =
    preferred ??
    Math.min(
      DRAWER_DEFAULT_MAX_PX,
      Math.max(DRAWER_MIN_PX, Math.round(viewportWidth * DRAWER_SHARE)),
    )
  return clampDrawerWidth(wanted, viewportWidth)
}

function subscribeResize(listener: () => void): () => void {
  window.addEventListener("resize", listener)
  return () => {
    window.removeEventListener("resize", listener)
  }
}

/** The window's inner width, re-read on resize; 0 without a window. */
export function useViewportWidth(): number {
  return useSyncExternalStore(
    subscribeResize,
    () => window.innerWidth,
    () => 0,
  )
}

function resolvePresentation(mode: CardOpenMode, fits: boolean): CardPresentation {
  if (mode === "page" || mode === "modal") return mode
  return fits ? "panel" : "modal"
}

export function useCardPresentation(): CardPresentation {
  const { cardOpenMode } = usePrefs()
  return resolvePresentation(cardOpenMode, drawerFits(useViewportWidth()))
}
