import { useEffect, useState } from "react"

/** Heading offset under the sticky top bar where a section counts as "current". */
const READING_LINE = 96

/** The id of the last TOC target whose top has scrolled above the reading line. */
export function useActiveSection(ids: readonly string[]): string | undefined {
  const [active, setActive] = useState<string>()
  useEffect(() => {
    if (ids.length === 0) return
    let frame = 0
    const measure = () => {
      frame = 0
      let current: string | undefined = ids[0]
      for (const id of ids) {
        const top = document.getElementById(id)?.getBoundingClientRect().top
        if (top === undefined || top > READING_LINE) break
        current = id
      }
      setActive(current)
    }
    const schedule = () => {
      if (!frame) frame = requestAnimationFrame(measure)
    }
    schedule()
    window.addEventListener("scroll", schedule, { passive: true })
    window.addEventListener("resize", schedule)
    // Content still laying out (fonts, the chunk rendering) moves headings without any scroll.
    const layout = typeof ResizeObserver === "undefined" ? undefined : new ResizeObserver(schedule)
    layout?.observe(document.body)
    return () => {
      layout?.disconnect()
      cancelAnimationFrame(frame)
      window.removeEventListener("scroll", schedule)
      window.removeEventListener("resize", schedule)
    }
  }, [ids])
  return active
}
