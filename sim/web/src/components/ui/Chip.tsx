import { X } from "lucide-react"
import type { ReactNode } from "react"

import { cn } from "./cn"

export interface ChipProps {
  readonly children: ReactNode
  /** Selected chips use the main text colour, never the brand colour (design decision 16). */
  readonly selected?: boolean
  readonly onClick?: () => void
  /** Renders a trailing × that calls this; the label must already be translated. */
  readonly onRemove?: (() => void) | undefined
  readonly removeLabel?: string
  readonly className?: string
}

// Invisible hit areas keep compact chips tappable (design §7).
export function Chip({
  children,
  selected = false,
  onClick,
  onRemove,
  removeLabel,
  className,
}: ChipProps) {
  const surface = cn(
    "relative inline-flex h-8 max-w-full items-center gap-1 rounded-pill border px-3 text-13 font-medium",
    selected ? "border-text-1 bg-surface-3 text-text-1" : "border-border bg-surface-1 text-text-2",
    "before:absolute before:inset-x-0 before:-inset-y-1.5 before:content-['']",
    className,
  )
  const label = <span className="min-w-0 truncate">{children}</span>
  if (onRemove) {
    // The remove button's hit area reaches past the chip's edge; that is not text overflow either.
    return (
      <span className={cn(surface, "pr-1")} data-hit-area="">
        {label}
        <button
          type="button"
          aria-label={removeLabel}
          data-hit-area=""
          onClick={onRemove}
          className="relative flex size-6 shrink-0 items-center justify-center rounded-pill before:absolute before:-inset-2.5 before:content-[''] hover:bg-surface-2"
        >
          <X className="size-3.5" aria-hidden="true" />
        </button>
      </span>
    )
  }
  if (onClick) {
    return (
      <button type="button" aria-pressed={selected} onClick={onClick} className={surface}>
        {label}
      </button>
    )
  }
  return <span className={surface}>{label}</span>
}
