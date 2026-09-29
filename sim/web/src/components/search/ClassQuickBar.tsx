import { useLayoutEffect, useRef, useState } from "react"

import { classStyle } from "../../domain/classes"
import { CLASS_ICON } from "../card/classIcons"
import { cn } from "../ui/cn"

export interface QuickBarClass {
  readonly code: string
  readonly label: string
}

export interface ClassQuickBarProps {
  readonly label: string
  readonly classes: readonly QuickBarClass[]
  readonly selected: readonly string[]
  readonly onToggle: (code: string) => void
  /** Test hook: decides whether the labelled row fits; defaults to measuring an off-screen copy. */
  readonly fits?: (labelledWidth: number, available: number) => boolean
}

const defaultFits = (labelled: number, available: number) => labelled <= available

// Design decision 19: seven cells always in one row, never scrolling; labels appear only when the
// labelled row fits the available width (measured, not guessed from the device).
export function ClassQuickBar({
  label,
  classes,
  selected,
  onToggle,
  fits = defaultFits,
}: ClassQuickBarProps) {
  const container = useRef<HTMLDivElement>(null)
  const probe = useRef<HTMLDivElement>(null)
  const [withText, setWithText] = useState(false)
  useLayoutEffect(() => {
    const target = container.current
    const measure = probe.current
    if (!target || !measure) return
    const update = () => {
      setWithText(fits(measure.scrollWidth, target.clientWidth))
    }
    update()
    const observer = new ResizeObserver(update)
    observer.observe(target)
    return () => {
      observer.disconnect()
    }
  }, [fits, classes])
  const cell = (item: QuickBarClass, text: boolean, interactive: boolean) => {
    const style = classStyle(item.code === "neutral" ? null : item.code)
    const known = item.code === "neutral" || style !== "neutral"
    const pressed = selected.includes(item.code)
    return (
      <button
        key={item.code}
        type="button"
        aria-pressed={interactive ? pressed : undefined}
        aria-label={text ? undefined : item.label}
        title={text ? undefined : item.label}
        tabIndex={interactive ? undefined : -1}
        onClick={
          interactive
            ? () => {
                onToggle(item.code)
              }
            : undefined
        }
        data-class={style}
        className={cn(
          "flex h-11 min-w-0 items-center justify-center gap-1.5 rounded-button border px-1 text-13 font-semibold whitespace-nowrap",
          pressed
            ? "border-text-1 bg-surface-3 text-text-1 ring-1 ring-text-1 ring-inset"
            : "border-border bg-surface-1 text-text-2 hover:bg-surface-2",
        )}
      >
        {known ? (
          <img src={CLASS_ICON[style]} alt="" className="size-6 shrink-0 object-contain" />
        ) : (
          <span className="flex size-6 shrink-0 items-center justify-center rounded-full bg-class-current text-12 font-bold text-surface-1">
            {item.label.slice(0, 1)}
          </span>
        )}
        {text && <span className="truncate">{item.label}</span>}
      </button>
    )
  }
  return (
    <div ref={container} role="group" aria-label={label} className="relative">
      <div
        className="grid gap-1.5"
        style={{ gridTemplateColumns: `repeat(${String(classes.length)}, minmax(0, 1fr))` }}
      >
        {classes.map((item) => cell(item, withText, true))}
      </div>
      {/* Off-screen copy with labels, measured to decide whether the real row may show text. */}
      <div aria-hidden="true" className="absolute top-0 left-0 size-0 overflow-hidden">
        <div ref={probe} className="flex w-max gap-1.5">
          {classes.map((item) => cell(item, true, false))}
        </div>
      </div>
    </div>
  )
}
