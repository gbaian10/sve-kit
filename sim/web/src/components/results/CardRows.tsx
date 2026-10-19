import {
  type CSSProperties,
  memo,
  type MouseEvent,
  type RefObject,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react"
import { useHref, useNavigate } from "react-router"

import { useAbilityPreview } from "../../app/snapshot"
import { type ImageSource } from "../../data"
import { classStyle } from "../../domain/classes"
import type { QueryState } from "../../domain/query/model"
import type { Region } from "../../domain/search"
import {
  LIST_MIN_WIDTHS,
  type ListColumn,
  type ListVisibility,
  prefsStore,
  usePrefs,
} from "../../settings"
import { CardTypeBadge } from "../card/CardTypeBadge"
import { CLASS_ICON } from "../card/classIcons"
import { DeckLocationBadges } from "../card/DeckLocationBadges"
import { InlineText } from "../card/InlineText"
import { RarityBadges } from "../card/RarityBadges"
import { RARITY_BADGE_HEIGHT, RARITY_COLUMN_MIN_WIDTH } from "../card/rarityBadgeSize"
import { StatLine } from "../card/StatLine"
import { ATTACK_ICON, DEFENSE_ICON } from "../card/symbolIcons"
import { Badge } from "../ui/Badge"
import { cn } from "../ui/cn"
import { ArtThumb } from "./ArtThumb"
import { type CardGridProps, type GridCell } from "./CardGrid"
import { type ResultLabels, useResultLabels } from "./labels"
import { activateCell } from "./plain-click"
import { useColumnReorder } from "./useColumnReorder"
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
  selected,
  className,
  children,
}: {
  readonly cell: GridCell
  readonly onOpen: CardGridProps["onOpen"]
  readonly highlighted: boolean
  readonly selected: boolean
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
      aria-current={selected ? "true" : undefined}
      onClick={(event: MouseEvent<HTMLAnchorElement>) => {
        activateCell(event, cell, onOpen, navigate)
      }}
      className={cn(
        "flex items-center gap-3 border-b border-border text-text-1 hover:bg-surface-2",
        highlighted && "ring-2 ring-text-1 ring-inset",
        selected && "bg-accent-soft ring-2 ring-accent ring-inset",
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
    <span role={phase === "pending" ? "status" : undefined} className="text-13 text-text-3 italic">
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
    </span>
  )
})

// Near-viewport previews limit text work while preserving the table layout (design 05, 02a).
const TableRow = memo(function TableRow({
  cell,
  images,
  onOpen,
  highlighted,
  selected,
  artOf,
  labels,
}: {
  readonly cell: GridCell
  readonly images: CardRowsProps["images"]
  readonly onOpen: CardRowsProps["onOpen"]
  readonly highlighted: boolean
  readonly selected: boolean
  readonly artOf: CardRowsProps["artOf"]
  readonly labels: ResultLabels
}) {
  const ref = useRef<HTMLLIElement>(null)
  const seen = useVisible(ref)
  return (
    <li ref={ref} className="min-w-0">
      <RowLink
        cell={cell}
        onOpen={onOpen}
        highlighted={highlighted}
        selected={selected}
        className="flex-wrap py-2"
      >
        <ArtThumb
          summary={cell.summary}
          name={cell.name.primary}
          images={images}
          art={artOf(cell)}
          labels={labels}
        />
        <span className="flex min-w-0 flex-1 flex-col gap-0.5 leading-tight">
          <span className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
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
            <span className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 text-11 text-text-3 tabular-nums">
              {cell.summary.cardNo}
              <RarityBadges {...cell.summary} />
              <DeckLocationBadges locations={cell.summary.deckLocations} />
              <CardTypeBadge code={cell.summary.typeCode} />
            </span>
          </span>
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
        <span className="min-w-0 basis-full pl-23">
          {seen ? (
            <EffectPreview
              printingId={cell.summary.printingId}
              faceId={cell.summary.faceId}
              edition={cell.summary.region}
              labels={labels}
            />
          ) : (
            <span className="flex min-w-0 flex-col gap-0.5" data-preview="waiting">
              <span className="text-13 text-text-3 italic">{labels.effectPending}</span>
            </span>
          )}
        </span>
      </RowLink>
    </li>
  )
})

export function CardTable({ cells, images, onOpen, anchor, selected, artOf }: CardRowsProps) {
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
          selected={selected === cell.key}
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
  readonly classLabel: (code: string | null) => string
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
  selected,
  labels,
  rowIndex,
  visibility,
  columns,
  classLabel,
}: {
  readonly cell: GridCell
  readonly onOpen: CardRowsProps["onOpen"]
  readonly highlighted: boolean
  readonly selected: boolean
  readonly labels: ResultLabels
  readonly rowIndex: number
  readonly visibility: ListVisibility
  readonly columns: readonly ListColumn[]
  readonly classLabel: string
}) {
  const row = useRef<HTMLTableRowElement>(null)
  const link = useRef<HTMLAnchorElement>(null)
  const seen = useVisible(row)
  const navigate = useNavigate()
  const href = useHref(cell.to)
  const navigationLink = (
    <a
      ref={link}
      href={href}
      data-result-key={cell.key}
      data-row-name=""
      lang={cell.name.primary.lang}
      title={cell.name.primary.text}
      aria-current={selected ? "true" : undefined}
      onClick={(event) => {
        activateCell(event, cell, onOpen, navigate)
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
      {visibility.name ? cell.name.primary.text : cell.summary.cardNo}
      {visibility.name && cell.name.secondary && (
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
  )
  return (
    <tr
      ref={row}
      aria-rowindex={rowIndex + 2}
      data-class={classStyle(cell.summary.classCode)}
      data-highlighted={highlighted || undefined}
      data-selected={selected || undefined}
      onClick={(event) => {
        if (event.target instanceof Element && event.target.closest("a")) return
        if (window.getSelection()?.isCollapsed === false) return
        link.current?.click()
      }}
    >
      {columns.map((column) => {
        switch (column) {
          case "number":
            return (
              <td key={column} data-list-number="">
                {visibility.name ? <span>{cell.summary.cardNo}</span> : navigationLink}
              </td>
            )
          case "rarity":
            return (
              <td key={column} data-list-rarity="">
                <RarityBadges {...cell.summary} />
              </td>
            )
          case "location":
            return (
              <td key={column} data-list-location="">
                <DeckLocationBadges locations={cell.summary.deckLocations} />
              </td>
            )
          case "type":
            return (
              <td key={column} data-list-type="">
                <CardTypeBadge code={cell.summary.typeCode} />
              </td>
            )
          case "name":
            return (
              <td key={column} data-list-name="">
                {navigationLink}
              </td>
            )
          case "class":
            return (
              <td key={column} data-list-class="">
                <img
                  src={CLASS_ICON[classStyle(cell.summary.classCode)]}
                  alt={classLabel}
                  title={classLabel}
                  className="mx-auto block size-6 object-contain"
                />
              </td>
            )
          case "cost":
            return (
              <td key={column} data-list-stat="">
                {cell.summary.cost ?? labels.noValue}
              </td>
            )
          case "attack":
            return (
              <td key={column} data-list-stat="">
                {cell.summary.attack ?? labels.noValue}
              </td>
            )
          case "defense":
            return (
              <td key={column} data-list-stat="">
                {cell.summary.defense ?? labels.noValue}
              </td>
            )
          case "effect":
            return (
              <td key={column} data-list-effect="">
                <span data-list-effect-text="">
                  {cell.summary.wordingPending && (
                    <Badge className="mr-2 align-middle">{labels.wordingPending}</Badge>
                  )}
                  {seen ? (
                    <EffectPreview
                      printingId={cell.summary.printingId}
                      faceId={cell.summary.faceId}
                      edition={cell.summary.region}
                      labels={labels}
                      singleLine
                    />
                  ) : (
                    <span data-preview="waiting" className="text-text-3 italic">
                      {labels.effectPending}
                    </span>
                  )}
                </span>
              </td>
            )
        }
      })}
    </tr>
  )
})

export function CardList({
  cells,
  onOpen,
  anchor,
  selected,
  sort = "no",
  descending = false,
  onSort,
  scrollPositionRef,
  footer,
  classLabel,
}: CardListProps) {
  const labels = useResultLabels()
  const scroll = useRef<HTMLDivElement>(null)
  const body = useRef<HTMLTableSectionElement>(null)
  const windowed = useRowWindow(body, cells.length)
  const { listVisibility, listWidths, listMobileWidths, listOrder } = usePrefs()
  const reorder = useColumnReorder("horizontal")
  const tableId = useId()
  const [overflow, setOverflow] = useState(false)
  useLayoutEffect(() => {
    const container = scroll.current
    const table = container?.querySelector("table")
    if (!container || !table || typeof ResizeObserver === "undefined") return
    const update = () => {
      setOverflow(table.offsetWidth > container.clientWidth)
    }
    const observer = new ResizeObserver(update)
    observer.observe(container)
    observer.observe(table)
    update()
    return () => {
      observer.disconnect()
    }
  }, [])
  useLayoutEffect(() => {
    if (!scroll.current || !scrollPositionRef) return
    scroll.current.scrollTop = scrollPositionRef.current.top
    scroll.current.scrollLeft = scrollPositionRef.current.left
  }, [overflow, scrollPositionRef])
  const columns = useMemo(
    () => listOrder.filter((column) => listVisibility[column]),
    [listOrder, listVisibility],
  )
  const names = {
    number: labels.t("results.columns.number"),
    rarity: labels.t("results.columns.rarity"),
    location: labels.t("results.columns.location"),
    type: labels.t("results.columns.type"),
    name: labels.t("results.columns.name"),
    class: labels.t("results.columns.class"),
    cost: labels.stats.cost,
    attack: labels.stats.attack,
    defense: labels.stats.defense,
    effect: labels.t("results.columns.effect"),
  }
  const sorts = { number: "no", name: "name", cost: "cost", attack: "atk", defense: "def" } as const
  const style = {
    "--class-column-min-width": `${String(LIST_MIN_WIDTHS.class)}px`,
    "--rarity-column-min-width": `${String(RARITY_COLUMN_MIN_WIDTH)}px`,
    "--rarity-badge-height": `${String(RARITY_BADGE_HEIGHT)}px`,
    ...Object.fromEntries(
      Object.entries(listWidths).map(([column, width]) => [
        `--list-user-${column}`,
        `${String(width)}px`,
      ]),
    ),
    ...Object.fromEntries(
      Object.entries(listMobileWidths).map(([column, width]) => [
        `--list-mobile-user-${column}`,
        `${String(width)}px`,
      ]),
    ),
    minWidth: `calc(${columns.map((column) => `var(--list-size-${column})`).join(" + ")})`,
  } as CSSProperties
  return (
    <div
      ref={scroll}
      data-list-scroll=""
      data-list-overflow={overflow || undefined}
      onScroll={(event) => {
        if (scrollPositionRef)
          scrollPositionRef.current = {
            top: event.currentTarget.scrollTop,
            left: event.currentTarget.scrollLeft,
          }
      }}
    >
      <table
        id={tableId}
        style={style}
        data-frozen-column={listVisibility.name ? "name" : "number"}
        data-view="list"
        data-card-list=""
        data-loaded-count={cells.length}
        aria-rowcount={cells.length + 1}
        aria-label={labels.t("results.listLabel")}
      >
        <colgroup>
          {columns.map((column) => (
            <col
              key={column}
              data-list-col={column}
              data-list-effect-col={column === "effect" ? "" : undefined}
            />
          ))}
        </colgroup>
        <thead>
          <tr aria-rowindex={1} data-column-order="">
            {columns.map((column) => {
              const key =
                column === "rarity" ||
                column === "location" ||
                column === "type" ||
                column === "effect" ||
                column === "class"
                  ? null
                  : sorts[column]
              const label = names[column]
              const title =
                column === "cost" ? (
                  <span
                    aria-hidden="true"
                    className="mx-auto block size-3 rounded-full bg-cost-dot ring-1 ring-text-1/30 ring-inset"
                  />
                ) : column === "attack" || column === "defense" ? (
                  <img
                    src={column === "attack" ? ATTACK_ICON : DEFENSE_ICON}
                    alt=""
                    draggable={false}
                    className="mx-auto size-4"
                  />
                ) : (
                  label
                )
              return (
                <th
                  key={column}
                  {...reorder.bindings(column)}
                  data-order-column={column}
                  data-drop={
                    reorder.insertion?.column === column
                      ? reorder.insertion.after
                        ? "after"
                        : "before"
                      : undefined
                  }
                  data-list-heading={column}
                  scope="col"
                  aria-label={label}
                  title={label}
                  aria-sort={
                    key === null
                      ? undefined
                      : sort === key
                        ? descending
                          ? "descending"
                          : "ascending"
                        : "none"
                  }
                >
                  {key === null ? (
                    <span data-list-heading-label="">{title}</span>
                  ) : (
                    <button
                      type="button"
                      aria-label={label}
                      title={label}
                      onClick={() => onSort?.(key)}
                    >
                      <span data-sort-label="">{title}</span>
                      <span data-sort-arrow="" aria-hidden="true">
                        {sort === key ? (descending ? " ↓" : " ↑") : ""}
                      </span>
                    </button>
                  )}
                  <ColumnResize
                    column={column}
                    label={labels.t("results.resizeColumn", { column: label })}
                    tableId={tableId}
                    width={listWidths[column]}
                  />
                </th>
              )
            })}
          </tr>
        </thead>
        <tbody ref={body}>
          {windowed.before > 0 && (
            <tr data-list-spacer="" aria-hidden="true">
              <td colSpan={columns.length} style={{ height: windowed.before }} />
            </tr>
          )}
          {cells.slice(windowed.start, windowed.end).map((cell, index) => (
            <ListRow
              key={cell.key}
              cell={cell}
              onOpen={onOpen}
              highlighted={anchor === cell.key}
              selected={selected === cell.key}
              labels={labels}
              rowIndex={windowed.start + index}
              visibility={listVisibility}
              columns={columns}
              classLabel={classLabel(cell.summary.classCode)}
            />
          ))}
          {windowed.after > 0 && (
            <tr data-list-spacer="" aria-hidden="true">
              <td colSpan={columns.length} style={{ height: windowed.after }} />
            </tr>
          )}
        </tbody>
      </table>
      {footer}
    </div>
  )
}

/* eslint-disable jsx-a11y/no-interactive-element-to-noninteractive-role -- Focusable ARIA separators support resizing; this rule only recognizes static separators. */
function ColumnResize({
  column,
  label,
  tableId,
  width,
}: {
  readonly column: ListColumn
  readonly label: string
  readonly tableId: string
  readonly width: number | undefined
}) {
  const drag = useRef<{
    pointer: number
    x: number
    width: number
    draft: number
    table: HTMLTableElement
    previous: string
    variable: string
    preference: "listWidths" | "listMobileWidths"
    moved: boolean
  } | null>(null)
  const handleRef = useRef<HTMLButtonElement>(null)
  const [measured, setMeasured] = useState<{ width: number; minimum: number } | null>(null)
  const widthPreference = (): "listWidths" | "listMobileWidths" =>
    window.matchMedia("(width < 600px)").matches ? "listMobileWidths" : "listWidths"
  const widthVariable = (preference: "listWidths" | "listMobileWidths") =>
    `--list-${preference === "listMobileWidths" ? "mobile-user" : "user"}-${column}`
  useLayoutEffect(() => {
    const heading = handleRef.current?.parentElement
    if (!heading) return
    const text =
      column === "rarity" || column === "class"
        ? heading.querySelector<HTMLElement>("[data-list-heading-label]")
        : null
    const update = () => {
      let minimum = LIST_MIN_WIDTHS[column]
      if (text) {
        const style = getComputedStyle(heading)
        const edges = [
          style.paddingLeft,
          style.paddingRight,
          style.borderLeftWidth,
          style.borderRightWidth,
        ].reduce((sum, value) => sum + (Number.parseFloat(value) || 0), 0)
        minimum = Math.max(minimum, Math.ceil(text.getBoundingClientRect().width + edges))
        heading
          .closest("table")
          ?.style.setProperty(`--${column}-column-min-width`, `${String(minimum)}px`)
      }
      const width = Math.round(heading.getBoundingClientRect().width)
      setMeasured((previous) =>
        previous?.width === width && previous.minimum === minimum ? previous : { width, minimum },
      )
    }
    update()
    if (typeof ResizeObserver === "undefined") return
    const observer = new ResizeObserver(update)
    observer.observe(heading)
    if (text) observer.observe(text)
    return () => {
      observer.disconnect()
    }
  }, [column, label])
  const minimum = measured?.minimum ?? LIST_MIN_WIDTHS[column]
  const clamp = (next: number) => Math.max(minimum, Math.round(next))
  const resize = (next: number, preference = widthPreference()) => {
    prefsStore.set({ [preference]: { ...prefsStore.get()[preference], [column]: clamp(next) } })
  }
  const cancel = () => {
    const active = drag.current
    if (!active) return
    drag.current = null
    if (active.previous) active.table.style.setProperty(active.variable, active.previous)
    else active.table.style.removeProperty(active.variable)
  }
  return (
    <button
      type="button"
      ref={handleRef}
      role="separator"
      tabIndex={0}
      aria-label={label}
      aria-orientation="vertical"
      aria-controls={tableId}
      aria-valuemin={minimum}
      aria-valuenow={measured?.width || width || minimum}
      data-column-resize={column}
      onKeyDown={(event) => {
        if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return
        event.preventDefault()
        const current = event.currentTarget.parentElement?.getBoundingClientRect().width ?? minimum
        resize(current + (event.key === "ArrowRight" ? 10 : -10))
      }}
      onPointerDown={(event) => {
        if (event.button !== 0) return
        const handle = event.currentTarget
        // Clear keyboard focus when the pointer takes over the same handle.
        handle.blur()
        const table = handle.closest("table")
        if (!table) return
        handle.setPointerCapture(event.pointerId)
        const width = handle.parentElement?.getBoundingClientRect().width ?? minimum
        const preference = widthPreference()
        const variable = widthVariable(preference)
        drag.current = {
          preference,
          variable,
          pointer: event.pointerId,
          x: event.clientX,
          width,
          draft: width,
          table,
          previous: table.style.getPropertyValue(variable),
          moved: false,
        }
      }}
      onPointerMove={(event) => {
        const active = drag.current
        if (active?.pointer !== event.pointerId) return
        active.draft = clamp(active.width + event.clientX - active.x)
        active.moved = true
        active.table.style.setProperty(active.variable, `${String(active.draft)}px`)
      }}
      onPointerUp={(event) => {
        const active = drag.current
        if (active?.pointer !== event.pointerId) return
        drag.current = null
        if (active.moved) resize(active.draft, active.preference)
        event.currentTarget.releasePointerCapture(event.pointerId)
      }}
      onPointerCancel={cancel}
      onLostPointerCapture={cancel}
    />
  )
}
/* eslint-enable jsx-a11y/no-interactive-element-to-noninteractive-role */
