import { cn } from "./cn"

export interface SwitchProps {
  readonly checked: boolean
  readonly onChange: (checked: boolean) => void
  readonly label: string
  readonly size?: "md" | "sm"
  readonly className?: string
}

// Design §7: 48×28 track (small 36×22); on = accent with an accent-ink knob, off = surface-3 with
// a strong border. The button itself is 44px tall so the touch target meets the phone minimum.
export function Switch({ checked, onChange, label, size = "md", className }: SwitchProps) {
  const track = size === "md" ? "h-7 w-12" : "h-5.5 w-9"
  const knob = size === "md" ? "size-5.5" : "size-4"
  const travel = size === "md" ? "translate-x-5" : "translate-x-3.5"
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => {
        onChange(!checked)
      }}
      className={cn("inline-flex h-11 min-w-11 shrink-0 items-center justify-center", className)}
    >
      <span
        aria-hidden="true"
        className={cn(
          "relative inline-flex items-center rounded-pill p-0.5 transition-colors",
          track,
          checked ? "bg-accent" : "border border-border-strong bg-surface-3",
        )}
      >
        <span
          className={cn(
            "block rounded-pill transition-transform",
            knob,
            checked ? cn("bg-accent-ink", travel) : "bg-text-3",
          )}
        />
      </span>
    </button>
  )
}
