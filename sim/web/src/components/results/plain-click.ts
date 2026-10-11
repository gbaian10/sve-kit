import type { MouseEvent } from "react"

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
