import { type RefObject, useLayoutEffect, useState } from "react"

const LIST_ROW_HEIGHT = 40
const OVERSCAN = 20

/** Fixed-height list rows keep their full scroll extent while only nearby rows mount readers. */
export function useRowWindow(bodyRef: RefObject<HTMLTableSectionElement | null>, count: number) {
  const [range, setRange] = useState({ start: 0, end: 60 })
  useLayoutEffect(() => {
    const body = bodyRef.current
    if (!body) return
    let frame: number | undefined
    const update = () => {
      frame = undefined
      const container = body.closest<HTMLElement>("[data-list-scroll]")
      const root = container && getComputedStyle(container).overflowY === "auto" ? container : null
      const top = body.getBoundingClientRect().top
      const viewport = root?.getBoundingClientRect()
      const first = Math.max(
        0,
        Math.floor(((viewport?.top ?? 0) - top) / LIST_ROW_HEIGHT) - OVERSCAN,
      )
      const last = Math.max(
        first,
        Math.ceil(((viewport?.bottom ?? window.innerHeight) - top) / LIST_ROW_HEIGHT) + OVERSCAN,
      )
      const start = Math.min(first, count)
      const end = Math.min(last, count)
      setRange((previous) =>
        previous.start === start && previous.end === end ? previous : { start, end },
      )
    }
    const schedule = () => {
      if (frame === undefined) frame = requestAnimationFrame(update)
    }
    update()
    window.addEventListener("scroll", schedule, true)
    window.addEventListener("resize", schedule)
    return () => {
      window.removeEventListener("scroll", schedule, true)
      window.removeEventListener("resize", schedule)
      if (frame !== undefined) cancelAnimationFrame(frame)
    }
  }, [bodyRef, count])
  const start = Math.min(range.start, count)
  const end = Math.max(start, Math.min(range.end, count))
  return { start, end, before: start * LIST_ROW_HEIGHT, after: (count - end) * LIST_ROW_HEIGHT }
}
