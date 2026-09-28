import { CircleAlert, CircleCheck, Info, TriangleAlert } from "lucide-react"
import type { ReactNode } from "react"

import { cn } from "./cn"

export type NoticeTone = "info" | "success" | "warning" | "danger"

// Design §7: 1px semantic border on the matching soft background, 13px text, 16px icon on the left.
const TONE: Record<NoticeTone, { className: string; Icon: typeof Info }> = {
  info: { className: "border-info bg-info-soft", Icon: Info },
  success: { className: "border-success bg-success-soft", Icon: CircleCheck },
  warning: { className: "border-warning bg-warning-soft", Icon: TriangleAlert },
  danger: { className: "border-danger bg-danger-soft", Icon: CircleAlert },
}

export function Notice({
  tone = "info",
  className,
  children,
}: {
  readonly tone?: NoticeTone
  readonly className?: string
  readonly children: ReactNode
}) {
  const { className: toneClass, Icon } = TONE[tone]
  return (
    <div
      role={tone === "danger" ? "alert" : "status"}
      className={cn(
        "flex items-start gap-2 rounded-control border px-3 py-2 text-13 text-text-1",
        toneClass,
        className,
      )}
    >
      <Icon className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
      <div className="min-w-0 flex-1">{children}</div>
    </div>
  )
}
