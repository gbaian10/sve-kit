import { type RefObject, useEffect, useLayoutEffect, useRef } from "react"

// Scroll position left on each history entry, so back lands where the reader was (in memory only:
// a reload starts every card at the top).
const leftAt = new Map<string, number>()
const MAX_ENTRIES = 200

/** The element the card drawer or modal scrolls in: the drawer marks it, the modal's is its parent. */
function scrollerOf(node: Element): HTMLElement | null {
  const marked = node.closest<HTMLElement>("[data-card-scroll]")
  return marked ?? node.closest("[data-card-overlay]")?.parentElement ?? null
}

/**
 * Scroll rules inside the card drawer or modal. A card reached by a link or by previous/next opens at
 * the top; going back to a history entry restores where it was left; another printing of the same
 * card keeps the position. Both cached and freshly downloaded cards pass here once their content is
 * shown (`pending` false), so the outcome does not depend on whether the data was already loaded.
 */
export function useFrameScroll({
  article,
  active,
  pending,
  cardId,
  entryKey,
}: {
  readonly article: RefObject<HTMLElement | null>
  readonly active: boolean
  readonly pending: boolean
  readonly cardId: string | undefined
  readonly entryKey: string
}) {
  const placed = useRef<{ entryKey: string; cardId: string } | null>(null)
  const shown = !pending && cardId !== undefined
  useLayoutEffect(() => {
    const node = article.current
    if (!active || !shown || !node || placed.current?.entryKey === entryKey) return
    const previous = placed.current
    placed.current = { entryKey, cardId }
    if (previous === null) return
    const scroller = scrollerOf(node)
    if (!scroller) return
    const saved = leftAt.get(entryKey)
    if (saved !== undefined) scroller.scrollTop = saved
    else if (previous.cardId !== cardId) scroller.scrollTop = 0
  }, [article, active, shown, entryKey, cardId])
  useEffect(() => {
    const node = article.current
    const scroller = node && active ? scrollerOf(node) : null
    if (!scroller) return
    const onScroll = () => {
      // While another card loads the old one is still on screen: its position is not this entry's.
      if (placed.current?.entryKey !== entryKey) return
      leftAt.delete(entryKey)
      leftAt.set(entryKey, scroller.scrollTop)
      if (leftAt.size > MAX_ENTRIES) leftAt.delete(leftAt.keys().next().value ?? "")
    }
    scroller.addEventListener("scroll", onScroll, { passive: true })
    return () => {
      scroller.removeEventListener("scroll", onScroll)
    }
  }, [article, active, entryKey])
}
