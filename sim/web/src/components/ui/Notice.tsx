import { CircleAlert, CircleCheck, Info, TriangleAlert } from "lucide-react"
import type { ReactNode } from "react"

import { cn } from "./cn"

type NoticeTone = "info" | "success" | "warning" | "danger"

export interface NoticeProps {
  readonly tone: NoticeTone
  readonly children: ReactNode
  /** Buttons or links placed at the end of the bar. */
  readonly actions?: ReactNode
  /** `alert` for failures that need attention, `status` for progress; omit for static text. */
  readonly role?: "alert" | "status"
  readonly className?: string
}

// Design §7: radius 10–12, 1px semantic border on the soft fill, 13px text, 16px leading icon. The
// icon carries the meaning alongside the colour, so the tone is never colour-only.
const TONE: Record<NoticeTone, { readonly box: string; readonly Icon: typeof Info }> = {
  info: { box: "border-info bg-info-soft [&>svg]:text-info", Icon: Info },
  success: { box: "border-success bg-success-soft [&>svg]:text-success", Icon: CircleCheck },
  warning: { box: "border-warning bg-warning-soft [&>svg]:text-warning", Icon: TriangleAlert },
  danger: { box: "border-danger bg-danger-soft [&>svg]:text-danger", Icon: CircleAlert },
}

export function Notice({ tone, children, actions, role, className }: NoticeProps) {
  const { box, Icon } = TONE[tone]
  return (
    <div
      role={role}
      className={cn(
        "flex flex-wrap items-center gap-x-3 gap-y-2 rounded-button border px-3 py-2.5 text-13 text-text-1",
        box,
        className,
      )}
    >
      <Icon className="size-4 shrink-0" aria-hidden="true" />
      <div className="min-w-0 flex-1 basis-40">{children}</div>
      {actions !== undefined && (
        <div className="ml-auto flex shrink-0 items-center gap-2">{actions}</div>
      )}
    </div>
  )
}
