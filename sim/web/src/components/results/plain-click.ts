import type { MouseEvent } from "react"
import type { NavigateFunction } from "react-router"

/** Plain-link click only; modified clicks (new tab, middle button) keep the browser's default. */
export function isPlainClick(event: MouseEvent<HTMLAnchorElement>): boolean {
  return !(
    event.defaultPrevented ||
    event.button !== 0 ||
    event.metaKey ||
    event.ctrlKey ||
    event.shiftKey ||
    event.altKey
  )
}

/** What a result link does after `onOpen`: push the card page, or nothing (the list opened the card in place). */
export type OpenOutcome =
  { readonly kind: "navigate"; readonly state: unknown } | { readonly kind: "opened" }

/** Shared by every result link: a plain click without a text selection opens the cell. */
export function activateCell<Cell extends { readonly to: string; readonly state: unknown }>(
  event: MouseEvent<HTMLAnchorElement>,
  cell: Cell,
  onOpen: (cell: Cell, event: MouseEvent<HTMLAnchorElement>) => OpenOutcome | undefined,
  navigate: NavigateFunction,
): void {
  if (!isPlainClick(event)) return
  event.preventDefault()
  if (event.detail > 0 && window.getSelection()?.isCollapsed === false) return
  const outcome = onOpen(cell, event)
  if (outcome?.kind === "opened") return
  void navigate(cell.to, { state: outcome?.state ?? cell.state })
}
