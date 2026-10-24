import { useEffect, useRef, useState } from "react"

/** Height of the sticky top bar the results header sticks under. */
const TOP_BAR_PX = 52
/** Height of the results header once it has folded into one row. */
const COMPACT_PX = 56
// Below this the count, chips and list controls cannot share a row across all UI languages.
const INLINE_MIN_PX = 900

// Preserve the full header's flow height while folded so the cards do not jump.
export function useCompactHeader(enabled: boolean) {
  const sentinelRef = useRef<HTMLDivElement>(null)
  const headRef = useRef<HTMLDivElement>(null)
  const [compact, setCompact] = useState(false)
  const [fullHeight, setFullHeight] = useState(0)
  const [narrow, setNarrow] = useState(true)
  useEffect(() => {
    const sentinel = sentinelRef.current
    if (!sentinel || !enabled || typeof IntersectionObserver === "undefined") return
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry) setCompact(!entry.isIntersecting && entry.boundingClientRect.top < TOP_BAR_PX)
      },
      { rootMargin: `-${String(TOP_BAR_PX)}px 0px 0px 0px` },
    )
    observer.observe(sentinel)
    return () => {
      observer.disconnect()
    }
  }, [enabled])
  const compactRef = useRef(false)
  useEffect(() => {
    compactRef.current = compact
  }, [compact])
  useEffect(() => {
    const head = headRef.current
    if (!head || !enabled || typeof ResizeObserver === "undefined") return
    const observer = new ResizeObserver(() => {
      setNarrow(head.clientWidth < INLINE_MIN_PX)
      if (!compactRef.current) setFullHeight(head.offsetHeight)
    })
    observer.observe(head)
    return () => {
      observer.disconnect()
    }
  }, [enabled])
  const foldedGap = compact ? Math.max(0, fullHeight - COMPACT_PX) : 0
  return { sentinelRef, headRef, compact: enabled && compact, narrow, foldedGap }
}
