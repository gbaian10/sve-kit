import type { ReactNode } from "react"

import { cn } from "./cn"

export type BadgeTone = "banned" | "limited" | "pending" | "neutral"

// Design §7: 22px tall, radius 5, 11px bold. Banned = danger on white, limited = warning on dark
// ink, pending = info soft, neutral (Token / evolve) = surface-3.
const TONE: Record<BadgeTone, string> = {
  banned: "bg-danger text-accent-ink",
  limited: "bg-warning text-accent-ink",
  pending: "bg-info-soft text-info",
  neutral: "bg-surface-3 text-text-1",
}

export function Badge({
  tone = "neutral",
  className,
  children,
}: {
  readonly tone?: BadgeTone
  readonly className?: string
  readonly children: ReactNode
}) {
  return (
    <span
      className={cn(
        "inline-flex h-5.5 items-center rounded-badge px-1.5 text-11 font-bold whitespace-nowrap",
        TONE[tone],
        className,
      )}
    >
      {children}
    </span>
  )
}
