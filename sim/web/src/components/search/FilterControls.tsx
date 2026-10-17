import { type ReactNode, useId } from "react"

import { cn } from "../ui/cn"

/** `mixed` marks a chip whose sub-values are only partly selected. */
export type TogglePressed = boolean | "mixed"

export function Toggle({
  pressed,
  disabled = false,
  label,
  title,
  onClick,
  className,
  children,
}: {
  readonly pressed: TogglePressed
  readonly disabled?: boolean
  /** Accessible name when the content is an image or differs from the spoken label. */
  readonly label?: string | undefined
  readonly title?: string | undefined
  readonly onClick: () => void
  readonly className?: string | undefined
  readonly children: ReactNode
}) {
  return (
    <button
      type="button"
      aria-pressed={pressed}
      aria-label={label}
      title={title}
      disabled={disabled}
      onClick={onClick}
      className={cn(
        "flex h-9 min-w-9 items-center justify-center rounded-control border px-2.5 text-13 font-semibold whitespace-nowrap tabular-nums",
        "disabled:cursor-not-allowed disabled:text-text-3",
        pressed === true
          ? "border-text-1 bg-surface-3 text-text-1 ring-1 ring-text-1 ring-inset"
          : pressed === "mixed"
            ? "border-dashed border-text-1 bg-surface-2 text-text-1"
            : "border-border bg-surface-1 text-text-2 hover:bg-surface-2",
        className,
      )}
    >
      {children}
    </button>
  )
}

export function Section({
  title,
  children,
}: {
  readonly title: string
  readonly children: ReactNode
}) {
  const id = useId()
  return (
    <div role="group" aria-labelledby={id} className="flex flex-col gap-1.5">
      <p id={id} className="text-12 font-semibold text-text-2">
        {title}
      </p>
      <div className="flex flex-wrap gap-1.5">{children}</div>
    </div>
  )
}
