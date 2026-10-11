import type { ReactNode } from "react"

import { cn } from "./cn"

type BadgeTone = "neutral" | "info" | "warning" | "danger"

export interface BadgeProps {
  readonly children: ReactNode
  readonly tone?: BadgeTone
  readonly className?: string
}

// Design §7: 22px tall, radius 5, 11px bold. Ban = danger on white, restriction = warning on dark
// ink, pending = info-soft, everything else surface-3 on the main text colour.
const TONE: Record<BadgeTone, string> = {
  neutral: "bg-surface-3 text-text-1",
  info: "bg-info-soft text-info",
  warning: "bg-warning text-accent-ink",
  danger: "bg-danger text-surface-1",
}

export function Badge({ children, tone = "neutral", className }: BadgeProps) {
  return (
    <span
      className={cn(
        "inline-flex h-5.5 shrink-0 items-center rounded-badge px-1.5 text-11 font-bold whitespace-nowrap",
        TONE[tone],
        className,
      )}
    >
      {children}
    </span>
  )
}
