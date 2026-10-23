import { ChevronDown } from "lucide-react"
import { type ReactNode, useCallback, useEffect, useId, useRef, useState } from "react"
import { useTranslation } from "react-i18next"

import { cn } from "../ui/cn"

/** Native popovers keep nested choices above the panel, including on touch screens. */
export function FilterDropdown({
  label,
  summary,
  selectedCount = 0,
  disabled = false,
  compact = false,
  onClear,
  children,
}: {
  readonly label: string
  readonly summary?: string
  readonly selectedCount?: number
  readonly disabled?: boolean
  readonly compact?: boolean
  readonly onClear?: (() => void) | undefined
  readonly children: ReactNode
}) {
  const { t } = useTranslation()
  const id = useId()
  const button = useRef<HTMLButtonElement>(null)
  const panel = useRef<HTMLDivElement>(null)
  const [open, setOpen] = useState(false)
  const place = useCallback(() => {
    const trigger = button.current
    const popup = panel.current
    if (!trigger || !popup || !popup.matches(":popover-open")) return
    if (trigger.closest("[hidden]")) {
      popup.hidePopover()
      return
    }
    // A split control anchors its submenu to the whole chip, including the arrow.
    const anchor = compact ? (trigger.closest("[data-filter-chip]") ?? trigger) : trigger
    const rect = anchor.getBoundingClientRect()
    const viewport = window.visualViewport
    const viewportRight = (viewport?.offsetLeft ?? 0) + (viewport?.width ?? window.innerWidth)
    const viewportTop = viewport?.offsetTop ?? 0
    const viewportBottom = viewportTop + (viewport?.height ?? window.innerHeight)
    const margin = 12
    const gap = 6
    const below = Math.max(0, viewportBottom - rect.bottom - margin - gap)
    const above = Math.max(0, rect.top - viewportTop - margin - gap)
    popup.style.width = `${String(Math.min(compact ? 320 : 480, Math.max(rect.width, viewportRight - rect.left - margin)))}px`
    popup.style.maxHeight = "none"
    const naturalHeight = popup.scrollHeight
    const upwards = naturalHeight > below && above > below
    popup.style.maxHeight = `${String(upwards ? above : below)}px`
    popup.style.left = `${String(rect.left)}px`
    popup.style.top = `${String(upwards ? rect.top - gap - popup.getBoundingClientRect().height : rect.bottom + gap)}px`
  }, [compact])
  useEffect(() => {
    if (!open) return
    const followAnchor = (event: Event) => {
      if (
        event.type === "scroll" &&
        event.target instanceof Node &&
        panel.current?.contains(event.target)
      )
        return
      place()
    }
    const observer = new ResizeObserver(place)
    if (button.current) {
      observer.observe(button.current)
      const chip = button.current.closest("[data-filter-chip]")
      if (chip) observer.observe(chip)
    }
    if (panel.current) observer.observe(panel.current)
    window.addEventListener("resize", place)
    window.addEventListener("scroll", followAnchor, true)
    window.visualViewport?.addEventListener("resize", place)
    window.visualViewport?.addEventListener("scroll", place)
    return () => {
      observer.disconnect()
      window.removeEventListener("resize", place)
      window.removeEventListener("scroll", followAnchor, true)
      window.visualViewport?.removeEventListener("resize", place)
      window.visualViewport?.removeEventListener("scroll", place)
    }
  }, [open, place])
  return (
    <div className={compact ? "inline-flex self-stretch" : "flex min-w-0 flex-col gap-2"}>
      {!compact && <p className="text-12 font-semibold text-text-2">{label}</p>}
      <button
        ref={button}
        type="button"
        disabled={disabled}
        aria-label={compact ? label : undefined}
        aria-expanded={open}
        aria-controls={id}
        popoverTarget={id}
        onClick={(event) => {
          event.preventDefault()
          const popup = panel.current
          if (!popup) return
          if (popup.matches(":popover-open")) {
            popup.hidePopover()
          } else {
            // Measure after showing, in the same event, before the browser paints.
            popup.showPopover({ source: event.currentTarget })
            place()
          }
        }}
        className={cn(
          "flex items-center border bg-surface-1 text-13 text-text-1 hover:bg-surface-2 disabled:cursor-not-allowed disabled:text-text-3",
          compact
            ? "h-full min-h-9 w-7 justify-center rounded-r-control border-l-0 border-border"
            : "min-h-11 w-full justify-between gap-2 rounded-control px-3 text-left",
          !compact && selectedCount > 0 ? "border-text-1" : "border-border",
        )}
      >
        {!compact && (
          <span className="min-w-0">
            <span className="sr-only">{label}: </span>
            <span className="block truncate">
              {summary ??
                (selectedCount > 0
                  ? t("rarityBadge.selected", { count: selectedCount })
                  : t("rarityBadge.all"))}
            </span>
          </span>
        )}
        <ChevronDown aria-hidden className={cn("size-4 shrink-0", open && "rotate-180")} />
      </button>
      <div
        ref={panel}
        id={id}
        popover="auto"
        role="group"
        aria-label={label}
        onToggle={(event) => {
          const shown = event.currentTarget.matches(":popover-open")
          setOpen(shown)
          if (shown) {
            place()
            event.currentTarget
              .querySelector<HTMLElement>("input, button")
              ?.focus({ preventScroll: true })
          }
        }}
        className="fixed inset-auto m-0 overflow-y-auto overscroll-contain rounded-block bg-surface-1 p-3 text-text-1 shadow-xl ring-1 ring-border"
      >
        <div className="mb-3 flex min-h-6 items-center justify-between gap-3 border-b border-border pb-2">
          <span className="text-13 font-semibold">{label}</span>
          {onClear && selectedCount > 0 && (
            <button
              type="button"
              aria-label={t("filters.clearSection", { section: label })}
              onClick={onClear}
              className="min-h-9 rounded-control px-2 text-13 font-semibold text-accent-text hover:bg-surface-2"
            >
              {t("filters.clear")}
            </button>
          )}
        </div>
        {children}
      </div>
    </div>
  )
}
