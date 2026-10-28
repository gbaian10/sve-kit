import {
  type ReactNode,
  type RefObject,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
} from "react"

export function CompactHeaderMenu({
  label,
  icon,
  children,
  fallbackFocusRef,
}: {
  readonly label: string
  readonly icon: ReactNode
  readonly children: ReactNode
  readonly fallbackFocusRef: RefObject<HTMLElement | null>
}) {
  const id = useId()
  const button = useRef<HTMLButtonElement>(null)
  const panel = useRef<HTMLDivElement>(null)
  const [open, setOpen] = useState(false)
  useLayoutEffect(
    () => () => {
      const popup = panel.current
      if (
        popup?.matches(":popover-open") &&
        (document.activeElement === document.body || popup.contains(document.activeElement))
      )
        fallbackFocusRef.current?.focus({ preventScroll: true })
    },
    [fallbackFocusRef],
  )
  useEffect(() => {
    if (!open) return
    const closeOnScroll = (event: Event) => {
      if (event.target instanceof Node && panel.current?.contains(event.target)) return
      panel.current?.hidePopover()
    }
    const observer =
      typeof IntersectionObserver === "undefined"
        ? undefined
        : new IntersectionObserver(([entry]) => {
            if (entry && !entry.isIntersecting) panel.current?.hidePopover()
          })
    if (button.current) observer?.observe(button.current)
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || !panel.current?.matches(":popover-open")) return
      // The card drawer must stay open when Escape dismisses its header menu.
      event.preventDefault()
      event.stopPropagation()
      panel.current.hidePopover()
      button.current?.focus()
    }
    window.addEventListener("keydown", onKeyDown, true)
    window.addEventListener("scroll", closeOnScroll, true)
    window.visualViewport?.addEventListener("scroll", closeOnScroll)
    return () => {
      observer?.disconnect()
      window.removeEventListener("keydown", onKeyDown, true)
      window.removeEventListener("scroll", closeOnScroll, true)
      window.visualViewport?.removeEventListener("scroll", closeOnScroll)
    }
  }, [open])
  return (
    <div className="shrink-0">
      <button
        ref={button}
        type="button"
        aria-label={label}
        aria-expanded={open}
        aria-controls={id}
        popoverTarget={id}
        onClick={(event) => {
          event.preventDefault()
          if (panel.current?.matches(":popover-open")) panel.current.hidePopover()
          else panel.current?.showPopover({ source: event.currentTarget })
        }}
        className="flex h-9 min-w-9 items-center justify-center gap-2 rounded-control border border-border bg-surface-1 px-2 text-13 text-text-1 hover:bg-surface-2"
      >
        {icon}
        <span className="@max-[640px]:hidden">{label}</span>
      </button>
      <div
        ref={panel}
        id={id}
        popover="auto"
        role="region"
        aria-label={label}
        onToggle={(event) => {
          const shown = event.currentTarget.matches(":popover-open")
          setOpen(shown)
          if (shown)
            event.currentTarget.querySelector<HTMLElement>("button, select, summary")?.focus()
        }}
        className="fixed inset-auto top-28 right-[calc(1rem+var(--drawer-width,0px)*var(--drawer-on-right,0))] m-0 max-h-[calc(100dvh-8rem)] w-96 max-w-[calc(100vw-2rem-var(--drawer-width,0px))] overflow-y-auto rounded-block border border-border bg-surface-1 p-3 text-text-1 shadow-lg"
      >
        {children}
      </div>
    </div>
  )
}
