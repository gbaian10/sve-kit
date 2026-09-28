import type { ComponentPropsWithoutRef } from "react"

import { cn } from "./cn"

export interface ChipProps extends ComponentPropsWithoutRef<"button"> {
  readonly selected?: boolean
}

// Design §7: 32px pill, surface-1 with a 1px border; selected = surface-3 with the text-colour ring,
// never the brand colour.
export function Chip({ selected = false, className, type = "button", ...rest }: ChipProps) {
  return (
    <button
      type={type}
      aria-pressed={selected}
      className={cn(
        "inline-flex h-8 shrink-0 items-center gap-1.5 rounded-pill border px-3 text-13 whitespace-nowrap",
        selected
          ? "border-text-1 bg-surface-3 text-text-1 ring-1 ring-text-1"
          : "border-border bg-surface-1 text-text-1 hover:bg-surface-2",
        className,
      )}
      {...rest}
    />
  )
}
