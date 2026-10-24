import { type KeyboardEvent, useId } from "react"

import { cn } from "./cn"
import { rovingRadioKeyDown } from "./useRovingRadio"

interface SegmentedOption<T extends string> {
  readonly value: T
  readonly label: string
}

export interface SegmentedProps<T extends string> {
  readonly label: string
  readonly options: readonly SegmentedOption<T>[]
  /** null = none of the options applies (a mixed setting): no radio is checked. */
  readonly value: T | null
  readonly onChange: (value: T) => void
  readonly disabled?: boolean
  readonly describedBy?: string | undefined
  /** `accent` is reserved for the view switcher (design §7); everything else keeps `neutral`. */
  readonly tone?: "neutral" | "accent"
  readonly size?: "sm" | "md"
  readonly className?: string
}

// Roving focus keeps the group to one Tab stop; invisible hit areas preserve compact pills.
export function Segmented<T extends string>({
  label,
  options,
  value,
  onChange,
  disabled = false,
  describedBy,
  tone = "neutral",
  size = "md",
  className,
}: SegmentedProps<T>) {
  const id = useId()
  const values = options.map((option) => option.value)
  const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    rovingRadioKeyDown(event, values, value ?? (values[0] as T), onChange, id)
  }
  return (
    <div
      role="radiogroup"
      aria-label={label}
      aria-describedby={describedBy}
      aria-disabled={disabled || undefined}
      className={cn(
        "inline-flex shrink-0 rounded-control border border-border bg-surface-1",
        size === "sm" ? "h-8.5 p-0.5" : "h-9 p-0.75",
        className,
      )}
    >
      {options.map((option) => {
        const checked = option.value === value
        return (
          <button
            key={option.value}
            id={`${id}-${option.value}`}
            type="button"
            role="radio"
            aria-checked={checked}
            disabled={disabled}
            tabIndex={checked || (value === null && option.value === values[0]) ? 0 : -1}
            onKeyDown={onKeyDown}
            onClick={() => {
              onChange(option.value)
            }}
            className={cn(
              "relative min-w-11 rounded-sm px-3 text-13 font-semibold whitespace-nowrap",
              "before:absolute before:inset-x-0 before:content-['']",
              size === "sm" ? "before:-inset-y-[7px]" : "before:-inset-y-1.5",
              checked
                ? tone === "accent"
                  ? "bg-accent text-accent-ink"
                  : "bg-surface-3 text-text-1 ring-1 ring-text-1"
                : "text-text-2 hover:text-text-1",
              disabled && "cursor-not-allowed text-text-3 hover:text-text-3",
            )}
          >
            {option.label}
          </button>
        )
      })}
    </div>
  )
}
