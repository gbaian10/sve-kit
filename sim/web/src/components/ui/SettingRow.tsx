import type { ReactNode } from "react"

import { cn } from "./cn"

export interface SettingRowProps {
  readonly label: string
  readonly hint?: string
  readonly size?: "sm" | "md"
  readonly className?: string
  readonly children: ReactNode
}

// Label and control share a line while both fit at their natural width; otherwise the control
// wraps under the label, right-aligned. Long translations therefore never squeeze the label into
// several lines or push the control past the container edge. The hint always takes a full line
// of its own, so its length never decides where the control goes.
export function SettingRow({ label, hint, size = "md", className, children }: SettingRowProps) {
  return (
    <div
      className={cn(
        "flex min-h-11 flex-wrap items-center justify-between gap-x-4 gap-y-1",
        className,
      )}
    >
      <div className={size === "sm" ? "text-14" : "text-15"}>{label}</div>
      <div className="ml-auto flex shrink-0 items-center">{children}</div>
      {hint && <div className="basis-full text-12 text-text-3">{hint}</div>}
    </div>
  )
}
