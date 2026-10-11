import { cn } from "../ui/cn"

export interface CardPlaceholderProps {
  /** Short state label under the mark, e.g. "no image yet"; omitted while loading. */
  readonly tag?: string
  /** Slow pulse while the image file index is still loading. */
  readonly pulse?: boolean
  readonly className?: string
}

// An original mark avoids using the official card back for unavailable images.
export function CardPlaceholder({ tag, pulse = false, className }: CardPlaceholderProps) {
  return (
    <div
      aria-hidden="true"
      className={cn(
        "absolute inset-0 flex flex-col items-center justify-center gap-[6%] bg-surface-2 text-text-3",
        pulse && "motion-safe:animate-pulse",
        className,
      )}
    >
      <svg viewBox="0 0 63 88" className="w-[46%] fill-none stroke-current" strokeWidth="2">
        <rect x="7" y="4" width="49" height="80" rx="6" className="stroke-border-strong" />
        <rect x="13" y="12" width="37" height="34" rx="3" className="fill-surface-3 stroke-none" />
        <path d="M31.5 20 40 30l-8.5 10L23 30z" className="fill-surface-1 stroke-border-strong" />
        <path
          d="M18 56h27M18 64h20M18 72h14"
          strokeLinecap="round"
          className="stroke-border-strong"
        />
      </svg>
      {tag !== undefined && (
        <span className="max-w-[90%] truncate rounded-badge bg-surface-3 px-1.5 py-0.5 text-center text-[min(3vw,11px)] text-text-2">
          {tag}
        </span>
      )}
    </div>
  )
}
