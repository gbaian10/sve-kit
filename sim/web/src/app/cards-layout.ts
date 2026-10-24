import { usePrefs } from "../settings"
import { useMediaQuery } from "./useMediaQuery"

/** From this width the cards page has a filter sidebar; below it filters open as a bottom sheet. */
const SIDEBAR_MIN_PX = 1180

/** With a card open, the sidebar stays unfolded only when the window fits it beside the drawer. */
export const BOTH_PANES_MIN_PX = 1700

export type DrawerSide = "left" | "right"

export function useSidebarLayout(): boolean {
  return useMediaQuery(`(min-width: ${String(SIDEBAR_MIN_PX)}px)`)
}

/** The card drawer's edge follows the layout preference wherever the drawer shows. */
export function useDrawerSide(): DrawerSide {
  return usePrefs().cardsLayout === "cardLeft" ? "left" : "right"
}
