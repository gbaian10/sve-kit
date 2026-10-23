import { type ReactNode, useId } from "react"
import { useTranslation } from "react-i18next"

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

// A section with a selection shows "clear" beside its title (the maintainer's choice over an
// "all" button: nothing selected already means all). The header keeps one fixed height so the
// button appearing or vanishing never moves the chips below.
export function Section({
  title,
  onClear,
  contentClassName,
  children,
}: {
  readonly title: string
  /** Clears this section only; given while it has a selection. */
  readonly onClear?: (() => void) | undefined
  readonly children: ReactNode
  readonly contentClassName?: string
}) {
  const { t } = useTranslation()
  const id = useId()
  return (
    <div role="group" aria-labelledby={id} className="flex min-w-0 flex-col gap-2">
      <div className="flex h-4 items-center gap-2 leading-4">
        <p id={id} className="text-12 font-semibold text-text-2">
          {title}
        </p>
        {onClear && (
          <button
            type="button"
            aria-label={t("filters.clearSection", { section: title })}
            onClick={onClear}
            className="h-4 rounded-sm px-1 text-12 font-semibold text-accent-text hover:bg-surface-2"
          >
            {t("filters.clear")}
          </button>
        )}
      </div>
      <div className={contentClassName ?? "flex flex-wrap gap-1.5"}>{children}</div>
    </div>
  )
}
