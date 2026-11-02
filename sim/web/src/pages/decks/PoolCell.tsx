import { Check, Crown } from "lucide-react"
import { useTranslation } from "react-i18next"

import type { GridCell } from "../../components/results/CardGrid"
import { cn } from "../../components/ui/cn"
import { SAME_NAME_LIMIT } from "../../domain/deck/model"
import { CountStepper } from "./CountStepper"
import { useDeckEditor } from "./deck-editor"

/**
 * What the deck builder adds over a pool card's image: the copies in the deck, and the side list
 * and leader marks, in the top-right corner (the cost sits top-left). Cards that cannot go into a
 * deck are dimmed; nothing covers the card otherwise.
 */
export function PoolCellOverlay({ cell }: { readonly cell: GridCell }) {
  const { t } = useTranslation()
  const status = useDeckEditor().statusOf(cell.summary.cardId)
  return (
    <>
      {status.placement === "none" && (
        <span aria-hidden="true" className="absolute inset-0 rounded-card bg-bg/40" />
      )}
      <span
        aria-hidden="true"
        className="absolute top-1.5 right-1.5 flex items-center gap-1 text-accent-ink"
      >
        {status.sideCount > 0 && (
          <span className="rounded-pill bg-surface-1/90 px-2 py-0.5 text-12 font-semibold text-text-1 ring-1 ring-border">
            {t("decks.cell.sideShort")}
          </span>
        )}
        {status.isLeader && (
          <span className="flex h-9 items-center gap-1 rounded-pill bg-accent px-2.5 text-14 font-bold shadow-md ring-2 ring-bg">
            <Crown className="size-4" />
            {t("decks.leader")}
          </span>
        )}
        {status.count > 0 && (
          <span className="flex h-9 min-w-9 items-center justify-center rounded-pill bg-accent px-2.5 text-18 leading-none font-extrabold tabular-nums shadow-md ring-2 ring-bg">
            {status.count}
          </span>
        )}
      </span>
    </>
  )
}

/** The stepper under a grid cell, or the compact one in a table or list row. */
export function PoolCellAction({
  cell,
  compact = false,
}: {
  readonly cell: GridCell
  readonly compact?: boolean
}) {
  const { t } = useTranslation()
  const editor = useDeckEditor()
  const { summary } = cell
  const status = editor.statusOf(summary.cardId)
  // A cell showing another printing than the card's default adds that printing.
  const printingId =
    editor.facts?.factsOf(summary.cardId)?.defaultPrintingId === summary.printingId
      ? undefined
      : summary.printingId
  const name = cell.name.primary.text
  if (status.placement === "leader")
    return (
      <button
        type="button"
        aria-pressed={status.isLeader}
        aria-label={t("decks.cell.setLeaderNamed", { name })}
        onClick={() => {
          editor.setLeader(status.isLeader ? null : summary.cardId, printingId)
        }}
        className={cn(
          "flex h-11 items-center justify-center gap-1 rounded-button border px-2 font-semibold",
          compact ? "text-12" : "w-full text-13",
          status.isLeader
            ? "border-accent bg-accent-soft text-accent-text"
            : "border-border bg-surface-1 text-text-1 hover:bg-surface-2",
        )}
      >
        {status.isLeader ? (
          <Check className="size-4" aria-hidden="true" />
        ) : (
          <Crown className="size-4" aria-hidden="true" />
        )}
        {status.isLeader ? t("decks.cell.isLeader") : t("decks.cell.setLeader")}
      </button>
    )
  if (status.placement === "none")
    return (
      <span className="flex h-11 items-center text-12 text-text-3">
        {compact ? t("decks.cell.notAllowed") : t("decks.cell.notAllowedHint")}
      </span>
    )
  return (
    <CountStepper
      count={status.count}
      max={Math.max(status.max, status.count)}
      limit={SAME_NAME_LIMIT}
      name={name}
      stretch={!compact}
      onChange={(count) => {
        if (count > status.count) editor.add(summary.cardId, printingId)
        else editor.remove(summary.cardId)
      }}
    />
  )
}
