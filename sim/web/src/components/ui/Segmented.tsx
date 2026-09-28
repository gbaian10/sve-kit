import { type KeyboardEvent, useId } from "react"

import { cn } from "./cn"
import { rovingRadioKeyDown } from "./useRovingRadio"

export interface SegmentedOption<T extends string> {
  readonly value: T
  readonly label: string
}

export interface SegmentedProps<T extends string> {
  readonly label: string
  readonly options: readonly SegmentedOption<T>[]
  readonly value: T
  readonly onChange: (value: T) => void
  /** `accent` is reserved for the view switcher (design §7); everything else keeps `neutral`. */
  readonly tone?: "neutral" | "accent"
  readonly size?: "sm" | "md"
  readonly className?: string
}

// A radiogroup: arrow keys move the selection, so a single Tab stop reaches the control. The
// visible control is 34/36px (design §7); an invisible ::before extends each radio's hit area to 44px.
export function Segmented<T extends string>({
  label,
  options,
  value,
  onChange,
  tone = "neutral",
  size = "md",
  className,
}: SegmentedProps<T>) {
  const id = useId()
  const values = options.map((option) => option.value)
  const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    rovingRadioKeyDown(event, values, value, onChange, id)
  }
  return (
    <div
      role="radiogroup"
      aria-label={label}
      className={cn(
        "inline-flex rounded-control border border-border bg-surface-1 p-0.5",
        size === "sm" ? "h-8.5" : "h-9",
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
            tabIndex={checked ? 0 : -1}
            onKeyDown={onKeyDown}
            onClick={() => {
              onChange(option.value)
            }}
            className={cn(
              "relative min-w-11 rounded-sm px-3 text-13 font-semibold",
              "before:absolute before:inset-x-0 before:content-['']",
              size === "sm" ? "before:-inset-y-[7px]" : "before:-inset-y-1.5",
              checked
                ? tone === "accent"
                  ? "bg-accent text-accent-ink"
                  : "bg-surface-3 text-text-1 ring-1 ring-text-1"
                : "text-text-2 hover:text-text-1",
            )}
          >
            {option.label}
          </button>
        )
      })}
    </div>
  )
}
