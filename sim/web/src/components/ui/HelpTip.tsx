import { type ReactNode, useEffect, useId, useRef, useState } from "react"

import { cn } from "./cn"

export interface HelpTipProps {
  /** Accessible name of the "?" button. */
  readonly label: string
  /** Rendered only while the panel is open, so closed tips cost nothing. */
  readonly children: ReactNode
  readonly className?: string
}

// A round "?" that explains a control by showing it. Mouse hover and keyboard focus open the
// panel; a tap (or click) pins it until another tap, a tap outside or Esc. The panel never takes
// pointer events, so it cannot cover the controls it sits over, and it is positioned against the
// nearest positioned ancestor (the SettingRow) so it stays inside a narrow screen.
export function HelpTip({ label, children, className }: HelpTipProps) {
  const panelId = useId()
  const root = useRef<HTMLSpanElement>(null)
  const pointerFocus = useRef(false)
  const [hover, setHover] = useState(false)
  const [focus, setFocus] = useState(false)
  const [pinned, setPinned] = useState(false)
  const open = hover || focus || pinned
  useEffect(() => {
    if (!open) return
    const close = () => {
      setHover(false)
      setFocus(false)
      setPinned(false)
    }
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") close()
    }
    const onPointerDown = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) close()
    }
    document.addEventListener("keydown", onKeyDown)
    document.addEventListener("pointerdown", onPointerDown)
    return () => {
      document.removeEventListener("keydown", onKeyDown)
      document.removeEventListener("pointerdown", onPointerDown)
    }
  }, [open])
  return (
    <span ref={root} className={cn("inline-flex", className)}>
      <button
        type="button"
        aria-label={label}
        aria-expanded={open}
        aria-describedby={open ? panelId : undefined}
        onPointerEnter={(event) => {
          if (event.pointerType === "mouse") setHover(true)
        }}
        onPointerLeave={() => {
          setHover(false)
        }}
        onPointerDown={() => {
          pointerFocus.current = true
        }}
        // Focus from a pointer is handled by the click; only keyboard focus opens the panel.
        onFocus={() => {
          if (!pointerFocus.current) setFocus(true)
          pointerFocus.current = false
        }}
        onBlur={() => {
          setFocus(false)
        }}
        onClick={() => {
          pointerFocus.current = false
          setFocus(false)
          setPinned((value) => !value)
          setHover(false)
        }}
        className="-mx-2.5 -my-3 inline-flex size-11 items-center justify-center rounded-pill text-text-3 hover:text-text-1"
      >
        <span
          aria-hidden="true"
          className={cn(
            "inline-flex size-5 items-center justify-center rounded-pill border text-12 leading-none font-semibold",
            open ? "border-accent bg-accent-soft text-text-1" : "border-border-strong",
          )}
        >
          ?
        </span>
      </button>
      {open && (
        <div
          id={panelId}
          role="tooltip"
          className="pointer-events-none absolute inset-x-0 top-full z-30 mt-1 max-h-[70vh] max-w-4xl overflow-hidden rounded-block border-2 border-accent bg-surface-2 p-3 text-14 shadow-2xl"
        >
          {children}
        </div>
      )}
    </span>
  )
}
