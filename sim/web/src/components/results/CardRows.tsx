import type { MouseEvent } from "react"
import { useTranslation } from "react-i18next"
import { useHref, useNavigate } from "react-router"

import { useAnnotatedText } from "../../app/snapshot"
import { type SnapshotClient, stringValue } from "../../data"
import { classStyle } from "../../domain/classes"
import type { Region } from "../../domain/search"
import { CardImage } from "../card/CardImage"
import { CLASS_ICON } from "../card/classIcons"
import { InlineText } from "../card/InlineText"
import { costIcon, costIconNeedsNumber } from "../card/symbolIcons"
import { useTextLanguage } from "../card/text-language"
import { Badge } from "../ui/Badge"
import { cn } from "../ui/cn"
import { type CardGridProps, type GridCell, NameNotices } from "./CardGrid"
import { isPlainClick } from "./plain-click"

export interface CardRowsProps extends CardGridProps {
  readonly client: SnapshotClient
  /** Revision owner per cell key, for the table's effect preview; absent = no preview row. */
  readonly effectOwner: (cell: GridCell) => { readonly kind: string; readonly id: string } | null
}

function Stat({ value, def = false }: { readonly value: number | null; readonly def?: boolean }) {
  const { t } = useTranslation()
  return (
    <span className={cn("text-14 font-semibold tabular-nums", def && "text-def")}>
      {value === null ? t("cardPage.noValue") : value}
    </span>
  )
}

function Cost({ value }: { readonly value: number }) {
  const { t } = useTranslation()
  return (
    <span className="flex shrink-0 items-center gap-1 text-14 font-semibold tabular-nums">
      <img src={costIcon(value)} alt={t("cardPage.costValue", { value })} className="size-5.5" />
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
  client,
  owner,
  edition,
}: {
  readonly client: SnapshotClient
  readonly owner: { readonly kind: string; readonly id: string }
  readonly edition: Region
}) {
  const { t } = useTranslation()
  const language = useTextLanguage()
  const { readiness, value } = useAnnotatedText(client, owner, "effect", language, edition)
  const unit = value ? (value.translated ?? value.original).unit : null
  return (
    <span className="flex min-w-0 flex-col gap-0.5">
      {unit ? (
        <span
          lang={stringValue(unit["lang"])}
          className="line-clamp-2 text-13 leading-snug text-text-2"
        >
          <InlineText text={stringValue(unit["text"])} />
        </span>
      ) : (
        <span
          role={readiness.basic === "pending" ? "status" : undefined}
          className="text-13 text-text-3"
        >
          {readiness.basic === "error"
            ? t("results.effectUnavailable")
            : t("results.effectPending")}
        </span>
      )}
      <span className="text-11 text-text-3">{t("results.abilityPreview")}</span>
    </span>
  )
}

// Table row (design 05, 02a): 48×67 thumbnail, name over translation, cost/attack/defense, then
// the two-line preview on its own line below so the row never grows past what the text needs.
export function CardTable({ cells, images, onOpen, anchor, client, effectOwner }: CardRowsProps) {
  const { t } = useTranslation()
  return (
    <ul data-view="table" className="flex flex-col">
      {cells.map((cell) => {
        const owner = effectOwner(cell)
        return (
          <li key={cell.key} className="min-w-0">
            <RowLink
              cell={cell}
              onOpen={onOpen}
              highlighted={anchor === cell.key}
              className="flex-wrap py-2"
            >
              <CardImage
                summary={cell.summary}
                name={cell.name.primary}
                images={images}
                alt="redundant"
                sizes="48px"
                className="w-12 shrink-0"
              />
              <span className="flex min-w-0 flex-1 flex-col leading-tight">
                <span lang={cell.name.primary.lang} className="truncate text-15 font-semibold">
                  {cell.name.primary.text}
                </span>
                {cell.name.secondary && (
                  <span lang={cell.name.secondary.lang} className="truncate text-13 text-text-2">
                    {cell.name.secondary.text}
                  </span>
                )}
                <span className="text-11 text-text-3 tabular-nums">{cell.summary.cardNo}</span>
                <NameNotices name={cell.name} />
                {cell.summary.wordingPending && (
                  <Badge className="mt-1 self-start">{t("card.wordingPending")}</Badge>
                )}
              </span>
              <span className="flex shrink-0 items-center gap-2">
                {cell.summary.cost !== null && <Cost value={cell.summary.cost} />}
                {cell.summary.attack !== null && cell.summary.defense !== null && (
                  <span className="flex items-baseline gap-0.5">
                    <Stat value={cell.summary.attack} />
                    <span className="text-13 text-text-3">/</span>
                    <Stat value={cell.summary.defense} def />
                  </span>
                )}
              </span>
              {owner && (
                <span className="basis-full pl-15">
                  <EffectPreview client={client} owner={owner} edition={cell.summary.region} />
                </span>
              )}
            </RowLink>
          </li>
        )
      })}
    </ul>
  )
}

// List row (design 05, 02a): 52px base, a 3px class-coloured edge, 22px cost icon, name over
// translation, attack/defense, chevron.
export function CardList({
  cells,
  onOpen,
  anchor,
}: Omit<CardRowsProps, "images" | "client" | "effectOwner">) {
  const { t } = useTranslation()
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
                <Cost value={cell.summary.cost} />
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
                <NameNotices name={cell.name} />
                {cell.summary.wordingPending && (
                  <Badge className="mt-0.5 self-start">{t("card.wordingPending")}</Badge>
                )}
              </span>
              {cell.summary.attack !== null && cell.summary.defense !== null && (
                <span className="flex shrink-0 items-baseline gap-0.5">
                  <Stat value={cell.summary.attack} />
                  <span className="text-13 text-text-3">/</span>
                  <Stat value={cell.summary.defense} def />
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
