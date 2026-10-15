import "./results.css"

import { memo, type MouseEvent } from "react"
import { useHref, useNavigate } from "react-router"

import type { CardSummary, ImageIndex } from "../../data"
import type { DisplayedName } from "../../domain/nameDisplay"
import { CardImage } from "../card/CardImage"
import { Badge } from "../ui/Badge"
import { cn } from "../ui/cn"
import { type ResultLabels, useResultLabels } from "./labels"
import { isPlainClick } from "./plain-click"

export interface GridCell {
  readonly key: string
  readonly summary: CardSummary
  readonly name: DisplayedName
  readonly to: string
  readonly state: unknown
}

export interface CardGridProps {
  readonly cells: readonly GridCell[]
  readonly images: ImageIndex | undefined
  /** Called before the router navigates, so the list can record its anchor. */
  readonly onOpen: (cell: GridCell, event: MouseEvent<HTMLAnchorElement>) => void
  /** Cell to highlight briefly after returning from a card. */
  readonly anchor?: string
}

/** Translation notices shared by the grid cell and the row shells; empty when there is none. */
export function NameNotices({
  name,
  labels,
}: {
  readonly name: DisplayedName
  readonly labels: ResultLabels
}) {
  if (!name.missingTranslation && !name.lowConfidence && !name.jpSource) return null
  return (
    <span className="flex flex-wrap gap-x-2 text-11 text-text-3">
      {name.missingTranslation && <span>{labels.noTranslation}</span>}
      {name.lowConfidence && <span>{labels.translationProofreading}</span>}
      {name.jpSource && <span>{labels.jpSource}</span>}
    </span>
  )
}

// Design: 2 columns under 600 (gap 14×12), 4 up to 999, 5–6 on desktop; each cell is the image
// plus one truncated 13px name line ("both" adds the translation in small type).
const GRID_SIZES =
  "(min-width: 1440px) 200px, (min-width: 1000px) 20vw, (min-width: 600px) 25vw, 50vw"

const CardCell = memo(function CardCell({
  cell,
  images,
  onOpen,
  highlighted,
  labels,
}: {
  readonly cell: GridCell
  readonly images: ImageIndex | undefined
  readonly onOpen: CardGridProps["onOpen"]
  readonly highlighted: boolean
  readonly labels: ResultLabels
}) {
  const navigate = useNavigate()
  const href = useHref(cell.to)
  const rarity = images?.asset(cell.summary.printingId, cell.summary.faceId)?.["rarity_code"]
  return (
    <a
      href={href}
      data-result-key={cell.key}
      onClick={(event) => {
        if (!isPlainClick(event)) return
        event.preventDefault()
        onOpen(cell, event)
        void navigate(cell.to, { state: cell.state })
      }}
      className={cn(
        "flex flex-col gap-1.5 rounded-card text-text-1 hover:opacity-90",
        highlighted && "ring-2 ring-text-1 ring-offset-2 ring-offset-bg",
      )}
    >
      <CardImage
        summary={cell.summary}
        name={cell.name.primary}
        images={images}
        alt="redundant"
        sizes={GRID_SIZES}
        className="w-full"
        translate={labels.t}
      />
      <span className="flex min-w-0 flex-col leading-tight">
        <span className="h-4 text-11 font-semibold text-text-3">
          {typeof rarity === "string" ? rarity.toUpperCase() : ""}
        </span>
        <span className="flex items-baseline gap-2">
          <span
            lang={cell.name.primary.lang}
            className="min-w-0 flex-1 truncate text-13 font-medium"
          >
            {cell.name.primary.text}
          </span>
          <span className="shrink-0 text-11 whitespace-nowrap text-text-3 tabular-nums">
            {cell.summary.cardNo}
          </span>
        </span>
        {cell.name.secondary && (
          <span lang={cell.name.secondary.lang} className="truncate text-11 text-text-3">
            {cell.name.secondary.text}
          </span>
        )}
        <NameNotices name={cell.name} labels={labels} />
        {cell.summary.wordingPending && (
          <Badge className="mt-1 self-start">{labels.wordingPending}</Badge>
        )}
      </span>
    </a>
  )
})

export function CardGrid({ cells, images, onOpen, anchor }: CardGridProps) {
  const labels = useResultLabels()
  return (
    // One column count drives both the grid and the off-screen height estimate in results.css.
    <ul
      data-view="grid"
      className="grid grid-cols-[repeat(var(--result-columns),minmax(0,1fr))] gap-x-3 gap-y-3.5 [--result-columns:2] md:[--result-columns:4] lg:[--result-columns:5] xl:[--result-columns:6]"
    >
      {cells.map((cell) => (
        <li key={cell.key} className="min-w-0">
          <CardCell
            cell={cell}
            images={images}
            onOpen={onOpen}
            highlighted={anchor === cell.key}
            labels={labels}
          />
        </li>
      ))}
    </ul>
  )
}
