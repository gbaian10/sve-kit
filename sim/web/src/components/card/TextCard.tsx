import { classStyle } from "../../domain/classes"
import type { DisplayedText } from "../../domain/nameDisplay"
import { cn } from "../ui/cn"
import { CLASS_ICON } from "./classIcons"

export interface TextCardProps {
  readonly name: DisplayedText
  readonly classCode: string | null
  readonly cost: number | null
  readonly attack: number | null
  readonly defense: number | null
  readonly cardNo: string
  /** Small label in the lower part, e.g. "no image yet". */
  readonly tag?: string
  /** Slow pulse while the first data is still loading. */
  readonly pulse?: boolean
  readonly className?: string
}

// The stand-in for a card image (design: loading, missing image, data saver all share it): the
// name, a thin class-coloured frame, cost top-left, class icon top-right, number and stats below.
// Type scales with the card (container units), so a 48px thumbnail does not overflow its bubbles.
export function TextCard({
  name,
  classCode,
  cost,
  attack,
  defense,
  cardNo,
  tag,
  pulse = false,
  className,
}: TextCardProps) {
  const style = classStyle(classCode)
  return (
    <div
      data-class={style}
      aria-hidden="true"
      className={cn(
        "@container absolute inset-0 flex flex-col rounded-card border-2 border-class-current bg-surface-1 p-[6%] text-text-1",
        pulse && "motion-safe:animate-pulse",
        className,
      )}
    >
      <div className="flex items-start justify-between">
        {cost === null ? (
          <span />
        ) : (
          <span className="flex size-[22%] items-center justify-center rounded-full bg-cost text-[min(12cqw,14px)] font-bold text-surface-1 tabular-nums">
            {cost}
          </span>
        )}
        <img src={CLASS_ICON[style]} alt="" className="size-[22%] object-contain" />
      </div>
      <span
        lang={name.lang}
        className="my-auto line-clamp-3 text-center text-[min(11cqw,13px)] leading-tight font-semibold wrap-anywhere"
      >
        {name.text}
      </span>
      {tag !== undefined && (
        <span className="mx-auto mb-1 rounded-badge bg-surface-3 px-1.5 py-0.5 text-[min(9.5cqw,11px)] text-text-2">
          {tag}
        </span>
      )}
      <div className="flex items-end justify-between text-[min(9.5cqw,11px)] text-text-3 tabular-nums">
        <span className="truncate">{cardNo}</span>
        {attack !== null && defense !== null && (
          <span>
            {attack}/<span className="text-def">{defense}</span>
          </span>
        )}
      </div>
    </div>
  )
}
