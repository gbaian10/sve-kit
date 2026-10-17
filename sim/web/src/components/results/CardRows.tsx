import { memo, type MouseEvent, type RefObject, useLayoutEffect, useRef } from "react"
import { useHref, useNavigate } from "react-router"

import { useAbilityPreview } from "../../app/snapshot"
import { type ImageSource } from "../../data"
import { classStyle } from "../../domain/classes"
import type { QueryState } from "../../domain/query/model"
import type { Region } from "../../domain/search"
import { InlineText } from "../card/InlineText"
import { StatLine } from "../card/StatLine"
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

interface ListScrollPosition {
  top: number
  left: number
}

interface CardListProps extends Omit<CardRowsProps, "images" | "artOf"> {
  readonly sort?: QueryState["sort"]
  readonly descending?: boolean
  readonly onSort?: (sort: QueryState["sort"]) => void
  readonly scrollPositionRef?: RefObject<ListScrollPosition>
}

function ListEffect({ cell, labels }: { readonly cell: GridCell; readonly labels: ResultLabels }) {
  const result = useAbilityPreview(
    cell.summary.printingId,
    cell.summary.faceId,
    labels.language,
    cell.summary.region,
  )
  if (result.phase !== "complete")
    return (
      <span
        data-preview={result.phase === "error" ? "error" : "pending"}
        role={result.phase === "pending" ? "status" : undefined}
      >
        {result.phase === "error" ? labels.effectUnavailable : labels.effectPending}
      </span>
    )
  return (
    <span data-preview="ready" lang={result.preview?.language}>
      {result.preview?.text ? (
        <InlineText text={result.preview.text.replace(/\s*\n\s*/gu, " ")} />
      ) : (
        labels.t("card.noEffect")
      )}
    </span>
  )
}

const ListRow = memo(function ListRow({
  cell,
  onOpen,
  highlighted,
  labels,
}: {
  readonly cell: GridCell
  readonly onOpen: CardRowsProps["onOpen"]
  readonly highlighted: boolean
  readonly labels: ResultLabels
}) {
  const row = useRef<HTMLTableRowElement>(null)
  const link = useRef<HTMLAnchorElement>(null)
  const seen = useSeen(row)
  const navigate = useNavigate()
  const href = useHref(cell.to)
  return (
    <tr
      ref={row}
      data-class={classStyle(cell.summary.classCode)}
      data-highlighted={highlighted || undefined}
      onClick={(event) => {
        if (event.target instanceof Element && event.target.closest("a")) return
        link.current?.click()
      }}
    >
      <td data-list-number="">
        <span>{cell.summary.cardNo}</span>
      </td>
      <td data-list-name="">
        <a
          ref={link}
          href={href}
          data-result-key={cell.key}
          lang={cell.name.primary.lang}
          title={cell.name.primary.text}
          onClick={(event) => {
            if (!isPlainClick(event)) return
            event.preventDefault()
            onOpen(cell, event)
            void navigate(cell.to, { state: cell.state })
          }}
          onKeyDown={(event) => {
            if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return
            const next =
              event.key === "ArrowDown"
                ? row.current?.nextElementSibling
                : row.current?.previousElementSibling
            const target = next?.querySelector<HTMLAnchorElement>("a[data-result-key]")
            if (target) {
              event.preventDefault()
              target.focus()
            }
          }}
        >
          {cell.name.primary.text}
        </a>
      </td>
      <td data-list-stat="">{cell.summary.cost ?? labels.noValue}</td>
      <td data-list-stat="">{cell.summary.attack ?? labels.noValue}</td>
      <td data-list-stat="">{cell.summary.defense ?? labels.noValue}</td>
      <td data-list-effect="">
        <span data-list-effect-text="">
          {cell.summary.wordingPending ? (
            <span>{labels.wordingPending}</span>
          ) : seen ? (
            <ListEffect cell={cell} labels={labels} />
          ) : (
            <span data-preview="waiting">{labels.effectPending}</span>
          )}
        </span>
      </td>
    </tr>
  )
})

export function CardList({
  cells,
  onOpen,
  anchor,
  sort = "no",
  descending = false,
  onSort,
  scrollPositionRef,
}: CardListProps) {
  const labels = useResultLabels()
  const scroll = useRef<HTMLDivElement>(null)
  useLayoutEffect(() => {
    if (!scroll.current || !scrollPositionRef) return
    scroll.current.scrollTop = scrollPositionRef.current.top
    scroll.current.scrollLeft = scrollPositionRef.current.left
  }, [scrollPositionRef])
  const columns = [
    ["no", labels.t("results.columns.number")],
    ["name", labels.t("results.columns.name")],
    ["cost", labels.stats.cost],
    ["atk", labels.stats.attack],
    ["def", labels.stats.defense],
  ] as const
  return (
    <div
      ref={scroll}
      data-list-scroll=""
      onScroll={(event) => {
        if (scrollPositionRef)
          scrollPositionRef.current = {
            top: event.currentTarget.scrollTop,
            left: event.currentTarget.scrollLeft,
          }
      }}
    >
      <table data-view="list" data-card-list="" aria-label={labels.t("results.view")}>
        <colgroup>
          <col data-list-number-col="" />
          <col data-list-name-col="" />
          <col span={3} data-list-stat-col="" />
          <col />
        </colgroup>
        <thead>
          <tr>
            {columns.map(([key, label]) => (
              <th
                key={key}
                scope="col"
                aria-sort={sort === key ? (descending ? "descending" : "ascending") : "none"}
              >
                <button type="button" onClick={() => onSort?.(key)}>
                  {label}
                  <span aria-hidden="true">{sort === key ? (descending ? " ↓" : " ↑") : ""}</span>
                </button>
              </th>
            ))}
            <th scope="col">{labels.t("results.columns.effect")}</th>
          </tr>
        </thead>
        <tbody>
          {cells.map((cell) => (
            <ListRow
              key={cell.key}
              cell={cell}
              onOpen={onOpen}
              highlighted={anchor === cell.key}
              labels={labels}
            />
          ))}
        </tbody>
      </table>
    </div>
  )
}
