import { useLayoutEffect, useRef, useState } from "react"
import { useTranslation } from "react-i18next"

import { clampDrawerWidth, DRAWER_MIN_PX, DRAWER_WIDTH_VAR } from "../../app/card-overlay"
import type { DrawerSide } from "../../app/cards-layout"
import { cn } from "../../components/ui/cn"
import { prefsStore } from "../../settings"

const KEY_STEP_PX = 40

/* eslint-disable jsx-a11y/no-interactive-element-to-noninteractive-role -- a focusable separator is how a resizable edge is exposed; the rule only knows static separators. */
/** The drawer's inner edge: drag or use the arrow keys to resize, saved as the drawer width preference. */
export function DrawerHandle({ side }: { readonly side: DrawerSide }) {
  const { t } = useTranslation()
  const ref = useRef<HTMLButtonElement>(null)
  const [width, setWidth] = useState<number | null>(null)
  const drag = useRef<{ pointer: number; width: number } | null>(null)
  useLayoutEffect(() => {
    const drawer = ref.current?.parentElement
    if (!drawer) return
    const update = () => {
      setWidth(Math.round(drawer.getBoundingClientRect().width))
    }
    update()
    if (typeof ResizeObserver === "undefined") return
    const observer = new ResizeObserver(update)
    observer.observe(drawer)
    return () => {
      observer.disconnect()
    }
  }, [])
  const preview = (next: number) => {
    document.documentElement.style.setProperty(DRAWER_WIDTH_VAR, `${String(next)}px`)
  }
  const commit = (next: number) => {
    prefsStore.set({ drawerWidth: next })
  }
  const label = t("cardPage.resizeDrawer")
  return (
    <button
      ref={ref}
      type="button"
      role="separator"
      tabIndex={0}
      aria-label={label}
      title={label}
      aria-orientation="vertical"
      aria-valuemin={DRAWER_MIN_PX}
      aria-valuenow={width ?? DRAWER_MIN_PX}
      className={cn(
        "w-1.5 shrink-0 cursor-col-resize touch-none bg-transparent hover:bg-accent focus-visible:bg-accent",
        side === "left" && "order-last",
      )}
      onKeyDown={(event) => {
        if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return
        event.preventDefault()
        const current = width || DRAWER_MIN_PX
        commit(
          clampDrawerWidth(
            current +
              ((event.key === "ArrowLeft") !== (side === "left") ? KEY_STEP_PX : -KEY_STEP_PX),
            window.innerWidth,
          ),
        )
      }}
      onPointerDown={(event) => {
        if (event.button !== 0) return
        event.currentTarget.setPointerCapture(event.pointerId)
        drag.current = { pointer: event.pointerId, width: width ?? DRAWER_MIN_PX }
      }}
      onPointerMove={(event) => {
        const active = drag.current
        if (active?.pointer !== event.pointerId) return
        active.width = clampDrawerWidth(
          side === "left" ? event.clientX : window.innerWidth - event.clientX,
          window.innerWidth,
        )
        preview(active.width)
      }}
      onPointerUp={(event) => {
        const active = drag.current
        if (active?.pointer !== event.pointerId) return
        drag.current = null
        event.currentTarget.releasePointerCapture(event.pointerId)
        commit(active.width)
      }}
      onPointerCancel={() => {
        drag.current = null
      }}
    />
  )
}
/* eslint-enable jsx-a11y/no-interactive-element-to-noninteractive-role */
