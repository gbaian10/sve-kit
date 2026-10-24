import type { ReactNode } from "react"

import { cn } from "./cn"

export interface SettingRowProps {
  readonly label: string
  readonly hint?: string
  /** A HelpTip beside the label; its panel opens under the row, so the row is its anchor. */
  readonly help?: ReactNode
  readonly size?: "sm" | "md"
  /** On a phone, always put the control on its own line under the label (many-option choices). */
  readonly stack?: boolean
  readonly className?: string
  readonly children: ReactNode
}

// Label and control share a line while both fit at their natural width; otherwise the control
// wraps under the label, right-aligned. Long translations therefore never squeeze the label into
// several lines or push the control past the container edge. The hint always takes a full line
// of its own, so its length never decides where the control goes.
export function SettingRow({
  label,
  hint,
  help,
  size = "md",
  stack = false,
  className,
  children,
}: SettingRowProps) {
  return (
    <div
      className={cn(
        "relative flex min-h-11 flex-wrap items-center justify-between gap-x-4 gap-y-1",
        stack && "max-md:flex-col max-md:items-stretch",
        className,
      )}
    >
      <div className={cn("flex items-center gap-1", size === "sm" ? "text-14" : "text-15")}>
        {label}
        {help}
      </div>
      <div
        className={cn(
          "ml-auto flex shrink-0 items-center",
          stack && "max-md:ml-0 max-md:justify-end",
        )}
      >
        {children}
      </div>
      {hint && <div className="basis-full text-12 text-text-3">{hint}</div>}
    </div>
  )
}
