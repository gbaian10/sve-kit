import { ChevronDown } from "lucide-react"
import { type ReactNode, useCallback, useEffect, useId, useRef, useState } from "react"
import { useTranslation } from "react-i18next"

import { cn } from "../ui/cn"

function scrollContainer(element: HTMLElement): HTMLElement | null {
  let parent = element.parentElement
  while (parent && parent !== document.documentElement) {
    const { overflow, overflowX, overflowY } = window.getComputedStyle(parent)
    if ([overflow, overflowX, overflowY].some((value) => /auto|scroll|overlay/u.test(value)))
      return parent
    parent = parent.parentElement
  }
  return null
}

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
    const boundary = scrollContainer(trigger)?.getBoundingClientRect()
    const viewportLeft = viewport?.offsetLeft ?? 0
    const viewportRight = viewportLeft + (viewport?.width ?? window.innerWidth)
    const viewportTop = viewport?.offsetTop ?? 0
    const viewportBottom = viewportTop + (viewport?.height ?? window.innerHeight)
    const margin = 12
    const gap = 6
    const left = Math.max(viewportLeft, boundary?.left ?? viewportLeft) + margin
    const right = Math.min(viewportRight, boundary?.right ?? viewportRight) - margin
    const top = Math.max(viewportTop, boundary?.top ?? viewportTop) + margin
    const bottom = Math.min(viewportBottom, boundary?.bottom ?? viewportBottom) - margin
    const width = Math.max(0, Math.min(compact ? 320 : 480, right - left))
    const below = Math.max(0, bottom - rect.bottom - gap)
    const above = Math.max(0, rect.top - top - gap)
    const popupLeft = Math.min(Math.max(rect.left, left), right - width)
    popup.style.width = `${String(width)}px`
    popup.style.maxHeight = "none"
    const naturalHeight = popup.scrollHeight
    const upwards = naturalHeight > below && above > below
    popup.style.maxHeight = `${String(upwards ? above : below)}px`
    popup.style.left = `${String(popupLeft)}px`
    popup.style.top = `${String(upwards ? rect.top - gap - popup.getBoundingClientRect().height : rect.bottom + gap)}px`
  }, [compact])
  useEffect(() => {
    if (!open) return
    const closeOnScroll = (event: Event) => {
      if (event.target instanceof Node && panel.current?.contains(event.target)) return
      panel.current?.hidePopover()
    }
    const observer =
      typeof IntersectionObserver === "undefined"
        ? undefined
        : new IntersectionObserver(
            ([entry]) => {
              if (entry && !entry.isIntersecting) panel.current?.hidePopover()
            },
            { root: scrollContainer(button.current ?? document.documentElement) },
          )
    if (button.current) observer?.observe(button.current)
    const resizeObserver = new ResizeObserver(place)
    if (button.current) {
      resizeObserver.observe(button.current)
      const chip = button.current.closest("[data-filter-chip]")
      if (chip) resizeObserver.observe(chip)
    }
    if (panel.current) resizeObserver.observe(panel.current)
    window.addEventListener("resize", place)
    window.addEventListener("scroll", closeOnScroll, true)
    window.visualViewport?.addEventListener("resize", place)
    window.visualViewport?.addEventListener("scroll", closeOnScroll)
    return () => {
      observer?.disconnect()
      resizeObserver.disconnect()
      window.removeEventListener("resize", place)
      window.removeEventListener("scroll", closeOnScroll, true)
      window.visualViewport?.removeEventListener("resize", place)
      window.visualViewport?.removeEventListener("scroll", closeOnScroll)
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
