import { type PointerEvent, useRef, useState } from "react"

import { type ListColumn, moveListColumn, prefsStore } from "../../settings"

interface Insertion {
  column: ListColumn
  after: boolean
}

export function useColumnReorder(axis: "horizontal" | "vertical") {
  const [insertion, setInsertion] = useState<Insertion | null>(null)
  const suppressClick = useRef(false)
  const drag = useRef<{
    pointer: number
    start: number
    source: ListColumn
    capture: HTMLElement
    moved: boolean
    insertion: Insertion | null
  } | null>(null)
  const coordinate = (event: PointerEvent) =>
    axis === "horizontal" ? event.clientX : event.clientY
  const cancel = () => {
    drag.current = null
    setInsertion(null)
  }
  const bindings = (column: ListColumn) => ({
    onPointerDown: (event: PointerEvent<HTMLElement>) => {
      if (event.button !== 0 || !event.isPrimary) return
      if (axis === "horizontal" && !window.matchMedia("(min-width: 600px)").matches) return
      if (event.target instanceof Element && event.target.closest("[data-column-resize]")) return
      suppressClick.current = false
      // Capture the original target so a short click still reaches its sort button.
      const capture = event.target instanceof HTMLElement ? event.target : event.currentTarget
      capture.setPointerCapture(event.pointerId)
      drag.current = {
        pointer: event.pointerId,
        start: coordinate(event),
        source: column,
        capture,
        moved: false,
        insertion: null,
      }
    },
    onPointerMove: (event: PointerEvent<HTMLElement>) => {
      const active = drag.current
      if (active?.pointer !== event.pointerId) return
      if (!active.moved && Math.abs(coordinate(event) - active.start) < 6) return
      if (!active.moved) {
        active.moved = true
        suppressClick.current = true
      }
      const container = event.currentTarget.closest("[data-column-order]")
      const targets = Array.from(
        container?.querySelectorAll<HTMLElement>("[data-order-column]") ?? [],
      )
      const position = coordinate(event)
      let next: Insertion | null = null
      for (const target of targets) {
        const rect = target.getBoundingClientRect()
        const start = axis === "horizontal" ? rect.left : rect.top
        const end = axis === "horizontal" ? rect.right : rect.bottom
        next = {
          column: target.dataset["orderColumn"] as ListColumn,
          after: position > (start + end) / 2,
        }
        if (position <= end) break
      }
      active.insertion = next
      setInsertion((previous) =>
        previous?.column === next?.column && previous?.after === next?.after ? previous : next,
      )
    },
    onPointerUp: (event: PointerEvent<HTMLElement>) => {
      const active = drag.current
      if (active?.pointer !== event.pointerId) return
      if (active.moved && active.insertion) {
        const order = prefsStore.get().listOrder
        const next = moveListColumn(
          order,
          active.source,
          active.insertion.column,
          active.insertion.after,
        )
        if (next.some((item, index) => item !== order[index])) prefsStore.set({ listOrder: next })
      }
      cancel()
      active.capture.releasePointerCapture(event.pointerId)
    },
    onPointerCancel: cancel,
    onLostPointerCapture: cancel,
    onClickCapture: (event: React.MouseEvent<HTMLElement>) => {
      if (suppressClick.current && event.detail !== 0) {
        event.preventDefault()
        event.stopPropagation()
        suppressClick.current = false
      }
    },
  })
  return { bindings, insertion }
}
