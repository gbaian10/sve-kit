import { Minus, Plus } from "lucide-react"
import { useTranslation } from "react-i18next"

import { cn } from "../../components/ui/cn"

// 44px buttons (design §7) that show a 36px face, so compact rows keep their look.
const BUTTON = "group flex size-11 shrink-0 items-center justify-center disabled:cursor-not-allowed"
const FACE =
  "flex size-9 items-center justify-center rounded-button border border-border bg-surface-1 text-text-1 group-hover:bg-surface-2 group-disabled:text-text-3 group-disabled:group-hover:bg-surface-1"

/** The explicit "− n ＋" control of every deck card. */
export function CountStepper({
  count,
  max,
  limit,
  onChange,
  name,
  stretch = false,
}: {
  readonly count: number
  /** ＋ stops here; omitted, it never stops (construction rules only warn, they never block). */
  readonly max?: number
  /** Shown after the count ("n/3"); the rule's limit, not what is left of it. */
  readonly limit?: number
  readonly onChange: (count: number) => void
  /** The card's name for the buttons' accessible names. */
  readonly name: string
  /** Fill the width (under a grid cell), with the count between the buttons. */
  readonly stretch?: boolean
}) {
  const { t } = useTranslation()
  return (
    <span
      className={cn(
        "inline-flex shrink-0 items-center tabular-nums",
        stretch && "w-full justify-between",
      )}
    >
      <button
        type="button"
        className={BUTTON}
        disabled={count <= 0}
        aria-label={t("decks.removeOne", { name })}
        onClick={() => {
          onChange(count - 1)
        }}
      >
        <span className={FACE}>
          <Minus className="size-4" aria-hidden="true" />
        </span>
      </button>
      <span
        className={cn(
          "min-w-7 flex-1 text-center text-14 font-semibold",
          count === 0 ? "text-text-3" : "text-text-1",
        )}
      >
        {count}
        {limit !== undefined && <span className="text-text-3">/{limit}</span>}
      </span>
      <button
        type="button"
        className={BUTTON}
        disabled={max !== undefined && count >= max}
        aria-label={t("decks.addOne", { name })}
        onClick={() => {
          onChange(count + 1)
        }}
      >
        <span className={FACE}>
          <Plus className="size-4" aria-hidden="true" />
        </span>
      </button>
    </span>
  )
}
