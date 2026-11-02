import { useLayoutEffect, useRef, useState } from "react"

export function useResultsHeaderWidth() {
  const headRef = useRef<HTMLDivElement>(null)
  const [width, setWidth] = useState(0)
  // Layout effect: measure before first paint so the header does not flash the narrow layout.
  useLayoutEffect(() => {
    const head = headRef.current
    if (!head) return
    const measure = () => {
      setWidth(head.clientWidth)
    }
    measure()
    if (typeof ResizeObserver === "undefined") return
    const observer = new ResizeObserver(measure)
    observer.observe(head)
    return () => {
      observer.disconnect()
    }
  }, [])
  return { headRef, width }
}
