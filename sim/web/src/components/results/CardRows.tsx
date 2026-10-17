import { memo, type MouseEvent, type RefObject, useLayoutEffect, useRef } from "react"
import { useHref, useNavigate } from "react-router"

import { useAbilityPreview } from "../../app/snapshot"
import { type ImageSource } from "../../data"
import { classStyle } from "../../domain/classes"
import type { QueryState } from "../../domain/query/model"
import type { Region } from "../../domain/search"
import { usePrefs } from "../../settings"
import { InlineText } from "../card/InlineText"
import { StatLine } from "../card/StatLine"
import { Badge } from "../ui/Badge"
import { cn } from "../ui/cn"
import { ArtThumb } from "./ArtThumb"
import { type CardGridProps, type GridCell, NameNotices } from "./CardGrid"
import { type ResultLabels, useResultLabels } from "./labels"
import { isPlainClick } from "./plain-click"
import { useRowWindow } from "./useRowWindow"
import { useVisible } from "./useVisible"

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

function PreviewStatus({
  phase,
  labels,
}: {
  readonly phase: string
  readonly labels: ResultLabels
}) {
  return (
    <span role={phase === "pending" ? "status" : undefined} className="text-13 text-text-3">
      {phase === "error" ? labels.effectUnavailable : labels.effectPending}
    </span>
  )
}

function OriginalPreview({
  printingId,
  faceId,
  edition,
  singleLine,
  labels,
}: {
  readonly printingId: string
  readonly faceId: string
  readonly edition: Region
  readonly singleLine: boolean
  readonly labels: ResultLabels
}) {
  const result = useAbilityPreview(printingId, faceId, edition === "jp" ? "ja" : "en", edition)
  return (
    <span
      data-original-preview={result.phase}
      lang={result.phase === "complete" ? result.preview?.language : undefined}
      className={singleLine ? undefined : "block truncate text-13 leading-snug text-text-2"}
    >
      {result.phase === "complete" ? (
        <InlineText text={result.preview?.text.replace(/\s*\n\s*/gu, " ") ?? ""} />
      ) : (
        <PreviewStatus phase={result.phase} labels={labels} />
      )}
    </span>
  )
}

// Both row views share language selection and resolve an original only when a translation exists.
const EffectPreview = memo(function EffectPreview({
  printingId,
  faceId,
  edition,
  labels,
  singleLine = false,
}: {
  readonly printingId: string
  readonly faceId: string
  readonly edition: Region
  readonly labels: ResultLabels
  readonly singleLine?: boolean
}) {
  const { effectLanguage } = usePrefs()
  const originalLanguage = edition === "jp" ? "ja" : "en"
  const result = useAbilityPreview(
    printingId,
    faceId,
    effectLanguage === "original" ? originalLanguage : labels.language,
    edition,
  )
  const unit = result.phase === "complete" ? result.preview : undefined
  const both =
    effectLanguage === "both" && Boolean(unit?.text) && unit?.language !== originalLanguage
  return (
    <span
      className={singleLine ? undefined : "flex min-w-0 flex-col gap-0.5"}
      data-preview={
        result.phase === "complete" ? "ready" : result.phase === "error" ? "error" : "pending"
      }
    >
      {result.phase === "complete" ? (
        <>
          <span
            lang={unit?.language}
            className={
              singleLine
                ? undefined
                : both
                  ? "block truncate text-13 leading-snug text-text-2"
                  : "line-clamp-2 text-13 leading-snug text-text-2"
            }
          >
            {unit?.text ? <InlineText text={unit.text.replace(/\s*\n\s*/gu, " ")} /> : null}
          </span>
          {both && (
            <>
              {singleLine && (
                <span
                  aria-hidden="true"
                  className="mx-2 inline-block h-3 border-l border-border-strong align-middle"
                />
              )}
              <OriginalPreview
                printingId={printingId}
                faceId={faceId}
                edition={edition}
                singleLine={singleLine}
                labels={labels}
              />
            </>
          )}
        </>
      ) : (
        <PreviewStatus phase={result.phase} labels={labels} />
      )}
      {!singleLine && <span className="text-11 text-text-3">{labels.abilityPreview}</span>}
    </span>
  )
})

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
  const seen = useVisible(ref)
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
            <span
              data-row-name=""
              lang={cell.name.primary.lang}
              className="min-w-0 truncate text-15 font-semibold"
            >
              {cell.name.primary.text}
              {cell.name.secondary && (
                <>
                  <span
                    aria-hidden="true"
                    className="mx-2 inline-block h-3 border-l border-border-strong align-middle"
                  />
                  <span lang={cell.name.secondary.lang} className="font-normal text-text-2">
                    {cell.name.secondary.text}
                  </span>
                </>
              )}
            </span>
            <span className="shrink-0 text-11 text-text-3 tabular-nums">{cell.summary.cardNo}</span>
          </span>
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
  readonly footer?: React.ReactNode
  readonly sort?: QueryState["sort"]
  readonly descending?: boolean
  readonly onSort?: (sort: QueryState["sort"]) => void
  readonly scrollPositionRef?: RefObject<ListScrollPosition>
}

const ListRow = memo(function ListRow({
  cell,
  onOpen,
  highlighted,
  labels,
  rowIndex,
}: {
  readonly cell: GridCell
  readonly onOpen: CardRowsProps["onOpen"]
  readonly highlighted: boolean
  readonly labels: ResultLabels
  readonly rowIndex: number
}) {
  const row = useRef<HTMLTableRowElement>(null)
  const link = useRef<HTMLAnchorElement>(null)
  const seen = useVisible(row)
  const navigate = useNavigate()
  const href = useHref(cell.to)
  return (
    <tr
      ref={row}
      aria-rowindex={rowIndex + 2}
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
          data-row-name=""
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
          {cell.name.secondary && (
            <>
              <span
                aria-hidden="true"
                className="mx-2 inline-block h-3 border-l border-border-strong align-middle"
              />
              <span lang={cell.name.secondary.lang} className="font-normal text-text-2">
                {cell.name.secondary.text}
              </span>
            </>
          )}
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
            <EffectPreview
              printingId={cell.summary.printingId}
              faceId={cell.summary.faceId}
              edition={cell.summary.region}
              labels={labels}
              singleLine
            />
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
  footer,
}: CardListProps) {
  const labels = useResultLabels()
  const scroll = useRef<HTMLDivElement>(null)
  const body = useRef<HTMLTableSectionElement>(null)
  const windowed = useRowWindow(body, cells.length)
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
      <table
        data-view="list"
        data-card-list=""
        data-loaded-count={cells.length}
        aria-rowcount={cells.length + 1}
        aria-label={labels.t("results.view")}
      >
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
        <tbody ref={body}>
          {windowed.before > 0 && (
            <tr data-list-spacer="" aria-hidden="true">
              <td colSpan={6} style={{ height: windowed.before }} />
            </tr>
          )}
          {cells.slice(windowed.start, windowed.end).map((cell, index) => (
            <ListRow
              key={cell.key}
              cell={cell}
              onOpen={onOpen}
              highlighted={anchor === cell.key}
              labels={labels}
              rowIndex={windowed.start + index}
            />
          ))}
          {windowed.after > 0 && (
            <tr data-list-spacer="" aria-hidden="true">
              <td colSpan={6} style={{ height: windowed.after }} />
            </tr>
          )}
        </tbody>
      </table>
      {footer}
    </div>
  )
}
