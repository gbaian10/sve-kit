import { ChevronDown, ChevronUp } from "lucide-react"
import { useState } from "react"
import { useTranslation } from "react-i18next"
import { useHref, useNavigate } from "react-router"

import type { EffectPreview, ImageIndex, TextContext } from "../../data"
import type { TextLang } from "../../domain/search"
import { CardImage } from "../card/CardImage"
import { CardText } from "../card/CardText"
import { symbolIcon } from "../card/symbolIcons"
import { cn } from "../ui/cn"
import type { GridCell } from "./CardGrid"

export interface TableViewProps {
  readonly cells: readonly GridCell[]
  readonly images: ImageIndex | undefined
  readonly onOpen: (cell: GridCell) => void
  readonly anchor?: string
  /** The effect text of a printing's front face, once loaded. */
  readonly preview: (printingId: string) => EffectPreview | undefined
  /** Icons and keyword names for the previews; plain text until it has loaded. */
  readonly context: TextContext | undefined
  readonly uiLang: TextLang
}

function Row({
  cell,
  images,
  onOpen,
  highlighted,
  preview,
  context,
  uiLang,
}: {
  readonly cell: GridCell
  readonly images: ImageIndex | undefined
  readonly onOpen: (cell: GridCell) => void
  readonly highlighted: boolean
  readonly preview: EffectPreview | undefined
  readonly context: TextContext | undefined
  readonly uiLang: TextLang
}) {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const href = useHref(cell.to)
  const [expanded, setExpanded] = useState(false)
  const costIcon =
    cell.summary.cost === null ? undefined : symbolIcon("cost", String(cell.summary.cost))
  const powerIcon = symbolIcon("power")
  const hpIcon = symbolIcon("hp")
  const stat = (icon: string | undefined, value: number | null) => (
    <span className="flex items-center gap-0.5 tabular-nums">
      {icon !== undefined && <img src={icon} alt="" className="size-4" />}
      <span className="text-14 font-semibold">{value ?? "—"}</span>
    </span>
  )
  return (
    <li
      className={cn(
        "flex gap-3 border-b border-border py-2",
        highlighted && "rounded-card ring-2 ring-accent",
      )}
    >
      <a
        href={href}
        data-result-key={cell.key}
        onClick={(event) => {
          if (
            event.defaultPrevented ||
            event.button !== 0 ||
            event.metaKey ||
            event.ctrlKey ||
            event.shiftKey ||
            event.altKey
          )
            return
          event.preventDefault()
          onOpen(cell)
          void navigate(cell.to, { state: cell.state })
        }}
        className="flex min-w-0 flex-1 items-start gap-3 text-text-1"
      >
        <CardImage
          summary={cell.summary}
          name={cell.name.primary}
          images={images}
          alt="redundant"
          sizes="64px"
          variant="art"
          className={cn(
            "shrink-0",
            images?.artImage(cell.summary.printingId, cell.summary.faceId) === undefined
              ? "w-12"
              : "w-16",
          )}
        />
        <span className="flex min-w-0 flex-1 flex-col gap-0.5">
          <span lang={cell.name.primary.lang} className="truncate text-15 font-semibold">
            {cell.name.primary.text}
          </span>
          {cell.name.secondary && (
            <span lang={cell.name.secondary.lang} className="truncate text-13 text-text-2">
              {cell.name.secondary.text}
            </span>
          )}
          <span className="flex gap-3 text-13 text-text-2">
            {stat(costIcon, cell.summary.cost)}
            {cell.summary.attack !== null && stat(powerIcon, cell.summary.attack)}
            {cell.summary.defense !== null && stat(hpIcon, cell.summary.defense)}
          </span>
          {preview !== undefined && (
            <span className={cn("text-13 text-text-2", !expanded && "line-clamp-2")}>
              {context ? (
                <CardText
                  compact
                  text={preview.text}
                  lang={preview.lang}
                  vocabulary={context.vocabulary}
                  symbolLocalization={context.symbolLocalization}
                  keyword={context.keyword}
                  uiLang={uiLang}
                  symbolLabels={false}
                />
              ) : (
                preview.text
              )}
            </span>
          )}
          {preview !== undefined && (
            <span className="text-11 text-text-3">{t("views.effectPreview")}</span>
          )}
        </span>
      </a>
      {preview !== undefined && (
        <button
          type="button"
          aria-expanded={expanded}
          aria-label={expanded ? t("views.collapse") : t("views.expand")}
          onClick={() => {
            setExpanded((value) => !value)
          }}
          className="flex size-11 shrink-0 items-center justify-center rounded-button text-text-2 hover:bg-surface-2"
        >
          {expanded ? (
            <ChevronUp className="size-5" aria-hidden="true" />
          ) : (
            <ChevronDown className="size-5" aria-hidden="true" />
          )}
        </button>
      )}
    </li>
  )
}

// Design §6.2 table view: 48×67 thumbnail, names, cost/attack/defense and a two-line preview.
export function TableView({
  cells,
  images,
  onOpen,
  anchor,
  preview,
  context,
  uiLang,
}: TableViewProps) {
  return (
    <ul>
      {cells.map((cell) => (
        <Row
          key={cell.key}
          cell={cell}
          images={images}
          onOpen={onOpen}
          highlighted={anchor === cell.key}
          preview={preview(cell.summary.printingId)}
          context={context}
          uiLang={uiLang}
        />
      ))}
    </ul>
  )
}
