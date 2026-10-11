import { memo, type MouseEvent, useRef } from "react"
import { useHref, useNavigate } from "react-router"

import { useAbilityPreview } from "../../app/snapshot"
import { type ImageSource } from "../../data"
import { classStyle } from "../../domain/classes"
import type { Region } from "../../domain/search"
import { CLASS_ICON } from "../card/classIcons"
import { InlineText } from "../card/InlineText"
import { StatLine } from "../card/StatLine"
import { costIcon, costIconNeedsNumber } from "../card/symbolIcons"
import { Badge } from "../ui/Badge"
import { cn } from "../ui/cn"
import { ArtThumb } from "./ArtThumb"
import { type CardGridProps, type GridCell, NameNotices } from "./CardGrid"
import { type ResultLabels, useResultLabels } from "./labels"
import { isPlainClick } from "./plain-click"
import { useSeen } from "./useSeen"

export interface CardRowsProps extends CardGridProps {
  /** The 4:3 illustration crop of a cell's face, once its image metadata is known. */
  readonly artOf: (cell: GridCell) => ImageSource | undefined
}

function Stat({
  value,
  def = false,
  labels,
}: {
  readonly value: number | null
  readonly def?: boolean
  readonly labels: ResultLabels
}) {
  return (
    <span className={cn("text-14 font-semibold tabular-nums", def && "text-def")}>
      {value === null ? labels.noValue : value}
    </span>
  )
}

function Cost({ value, labels }: { readonly value: number; readonly labels: ResultLabels }) {
  return (
    <span className="flex shrink-0 items-center gap-1 text-14 font-semibold tabular-nums">
      <img
        src={costIcon(value)}
        alt={labels.t("cardPage.costValue", { value })}
        className="size-5.5"
      />
      {costIconNeedsNumber(value) && <span aria-hidden="true">{value}</span>}
    </span>
  )
}

function RowLink({
  cell,
  onOpen,
  highlighted,
  className,
  children,
}: {
  readonly cell: GridCell
  readonly onOpen: CardGridProps["onOpen"]
  readonly highlighted: boolean
  readonly className: string
  readonly children: React.ReactNode
}) {
  const navigate = useNavigate()
  const href = useHref(cell.to)
  return (
    <a
      href={href}
      data-result-key={cell.key}
      data-class={classStyle(cell.summary.classCode)}
      onClick={(event: MouseEvent<HTMLAnchorElement>) => {
        if (!isPlainClick(event)) return
        event.preventDefault()
        onOpen(cell, event)
        void navigate(cell.to, { state: cell.state })
      }}
      className={cn(
        "flex items-center gap-3 border-b border-border text-text-1 hover:bg-surface-2",
        highlighted && "ring-2 ring-text-1 ring-inset",
        className,
      )}
    >
      {children}
    </a>
  )
}

// Two-line ability preview (design 05): the effect in the UI language, clamped, with a label
// that says it is a preview; the full text lives on the card page.
function EffectPreview({
  printingId,
  faceId,
  edition,
  labels,
}: {
  readonly printingId: string
  readonly faceId: string
  readonly edition: Region
  readonly labels: ResultLabels
}) {
  const result = useAbilityPreview(printingId, faceId, labels.language, edition)
  const unit = result.phase === "complete" ? result.preview : undefined
  return (
    <span
      className="flex min-w-0 flex-col gap-0.5"
      data-preview={
        result.phase === "complete" ? "ready" : result.phase === "error" ? "error" : "pending"
      }
    >
      {result.phase === "complete" ? (
        <span lang={unit?.language} className="line-clamp-2 text-13 leading-snug text-text-2">
          <InlineText text={unit?.text ?? ""} />
        </span>
      ) : (
        <span
          role={result.phase === "pending" ? "status" : undefined}
          className="text-13 text-text-3"
        >
          {result.phase === "error" ? labels.effectUnavailable : labels.effectPending}
        </span>
      )}
      <span className="text-11 text-text-3">{labels.abilityPreview}</span>
    </span>
  )
}

// Near-viewport previews limit text work while preserving the table layout (design 05, 02a).
const TableRow = memo(function TableRow({
  cell,
  images,
  onOpen,
  highlighted,
  artOf,
  labels,
}: {
  readonly cell: GridCell
  readonly images: CardRowsProps["images"]
  readonly onOpen: CardRowsProps["onOpen"]
  readonly highlighted: boolean
  readonly artOf: CardRowsProps["artOf"]
  readonly labels: ResultLabels
}) {
  const ref = useRef<HTMLLIElement>(null)
  const seen = useSeen(ref)
  return (
    <li ref={ref} className="min-w-0">
      <RowLink cell={cell} onOpen={onOpen} highlighted={highlighted} className="flex-wrap py-2">
        <ArtThumb
          summary={cell.summary}
          name={cell.name.primary}
          images={images}
          art={artOf(cell)}
          labels={labels}
        />
        <span className="flex min-w-0 flex-1 flex-col gap-0.5 leading-tight">
          <span className="flex items-baseline gap-2">
            <span lang={cell.name.primary.lang} className="min-w-0 truncate text-15 font-semibold">
              {cell.name.primary.text}
            </span>
            <span className="shrink-0 text-11 text-text-3 tabular-nums">{cell.summary.cardNo}</span>
          </span>
          {cell.name.secondary && (
            <span lang={cell.name.secondary.lang} className="truncate text-13 text-text-2">
              {cell.name.secondary.text}
            </span>
          )}
          <NameNotices name={cell.name} labels={labels} />
          <StatLine
            labels={labels.stats}
            size="sm"
            cost={cell.summary.cost}
            attack={cell.summary.attack}
            defense={cell.summary.defense}
          />
          {cell.summary.wordingPending && (
            <Badge className="mt-0.5 self-start">{labels.wordingPending}</Badge>
          )}
        </span>
        <span className="basis-full pl-23">
          {seen ? (
            <EffectPreview
              printingId={cell.summary.printingId}
              faceId={cell.summary.faceId}
              edition={cell.summary.region}
              labels={labels}
            />
          ) : (
            <span className="flex min-w-0 flex-col gap-0.5" data-preview="waiting">
              <span className="text-13 text-text-3">{labels.effectPending}</span>
              <span className="text-11 text-text-3">{labels.abilityPreview}</span>
            </span>
          )}
        </span>
      </RowLink>
    </li>
  )
})

export function CardTable({ cells, images, onOpen, anchor, artOf }: CardRowsProps) {
  const labels = useResultLabels()
  return (
    <ul data-view="table" className="flex flex-col">
      {cells.map((cell) => (
        <TableRow
          key={cell.key}
          cell={cell}
          images={images}
          onOpen={onOpen}
          highlighted={anchor === cell.key}
          artOf={artOf}
          labels={labels}
        />
      ))}
    </ul>
  )
}

// List row (design 05, 02a): 52px base, a 3px class-coloured edge, 22px cost icon, name over
// translation, attack/defense, chevron.
export function CardList({ cells, onOpen, anchor }: Omit<CardRowsProps, "images" | "artOf">) {
  const labels = useResultLabels()
  return (
    <ul data-view="list" className="flex flex-col">
      {cells.map((cell) => {
        const style = classStyle(cell.summary.classCode)
        return (
          <li key={cell.key} className="min-w-0">
            <RowLink
              cell={cell}
              onOpen={onOpen}
              highlighted={anchor === cell.key}
              className="min-h-13 border-l-3 border-l-class-current py-1 pr-2 pl-3"
            >
              {cell.summary.cost !== null ? (
                <Cost value={cell.summary.cost} labels={labels} />
              ) : (
                <img src={CLASS_ICON[style]} alt="" className="size-5.5 shrink-0 object-contain" />
              )}
              <span className="flex min-w-0 flex-1 flex-col leading-tight">
                <span className="flex items-baseline gap-2">
                  <span
                    lang={cell.name.primary.lang}
                    className="min-w-0 truncate text-15 font-semibold"
                  >
                    {cell.name.primary.text}
                  </span>
                  <span className="shrink-0 text-11 text-text-3 tabular-nums">
                    {cell.summary.cardNo}
                  </span>
                </span>
                {cell.name.secondary && (
                  <span lang={cell.name.secondary.lang} className="truncate text-12 text-text-2">
                    {cell.name.secondary.text}
                  </span>
                )}
                <NameNotices name={cell.name} labels={labels} />
                {cell.summary.wordingPending && (
                  <Badge className="mt-0.5 self-start">{labels.wordingPending}</Badge>
                )}
              </span>
              {cell.summary.attack !== null && cell.summary.defense !== null && (
                <span className="flex shrink-0 items-baseline gap-0.5">
                  <Stat value={cell.summary.attack} labels={labels} />
                  <span className="text-13 text-text-3">/</span>
                  <Stat value={cell.summary.defense} def labels={labels} />
                </span>
              )}
              <svg
                aria-hidden="true"
                viewBox="0 0 24 24"
                className="size-4 shrink-0 text-text-3"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
              >
                <path d="m9 5 7 7-7 7" />
              </svg>
            </RowLink>
          </li>
        )
      })}
    </ul>
  )
}
