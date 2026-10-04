import { useTranslation } from "react-i18next"

import type { CardSummary, ImageIndex } from "../../data"
import type { DisplayedName } from "../../domain/nameDisplay"
import type { MatchedField } from "../../domain/search"
import { CardImage } from "../card/CardImage"
import { cn } from "../ui/cn"
import { optionId } from "./suggest-ids"

export interface SuggestRow {
  readonly summary: CardSummary
  readonly name: DisplayedName
  /** Undefined for recent cards, which have no matched field. */
  readonly field?: MatchedField
}

export interface SuggestListProps {
  readonly id: string
  readonly rows: readonly SuggestRow[]
  readonly activeIndex: number
  readonly images: ImageIndex | undefined
  readonly onPick: (index: number) => void
  readonly onHover: (index: number) => void
  /** The rows belong to an older text than the input holds; they stay visible but cannot be picked. */
  readonly stale?: boolean
  /** Suggestions mode: the query and the total behind "see all". Recent mode when undefined. */
  readonly query?: { readonly text: string; readonly total: number; readonly onSeeAll: () => void }
  readonly onClearRecent?: () => void
}

// Design 04: 56px rows of thumbnail 32×44, the name, then "translation · number · matched field";
// while the input is empty the same list shows the recently viewed cards.
export function SuggestList({
  id,
  rows,
  activeIndex,
  images,
  onPick,
  onHover,
  stale = false,
  query,
  onClearRecent,
}: SuggestListProps) {
  const { t } = useTranslation()
  const heading = query ? t("search.suggestions") : t("search.recent")
  return (
    <div className="rounded-block border border-border bg-surface-1 shadow-lg">
      <div className="flex items-baseline justify-between px-4 pt-2 pb-1 text-12 text-text-3">
        <span>{heading}</span>
        {query && query.total > 0 && (
          <button
            type="button"
            onMouseDown={(event) => {
              event.preventDefault()
            }}
            onClick={query.onSeeAll}
            className="text-accent-text hover:underline"
          >
            {t("search.seeAll", { count: query.total })}
          </button>
        )}
        {!query && rows.length > 0 && onClearRecent && (
          <button
            type="button"
            onMouseDown={(event) => {
              event.preventDefault()
            }}
            onClick={onClearRecent}
            className="text-accent-text hover:underline"
          >
            {t("search.clearRecent")}
          </button>
        )}
      </div>
      {rows.length === 0 ? (
        <p className="px-4 pt-1 pb-3 text-13 leading-relaxed text-text-3">
          {query ? t("search.noSuggestions") : t("search.noRecent")}
        </p>
      ) : (
        <div
          id={id}
          role="listbox"
          aria-label={heading}
          aria-busy={stale}
          className={cn("pb-1", stale && "opacity-60")}
        >
          {rows.map((row, index) => {
            const active = index === activeIndex
            const details = [
              row.name.secondary?.text,
              row.name.missingTranslation ? t("card.noTranslation") : undefined,
              row.summary.cardNo,
              row.field === undefined
                ? undefined
                : t("search.matched", { field: t(`search.field.${row.field}`) }),
            ].filter((part): part is string => part !== undefined)
            return (
              <button
                key={row.summary.printingId}
                type="button"
                id={optionId(id, index)}
                role="option"
                aria-selected={active}
                aria-disabled={stale}
                tabIndex={-1}
                onMouseDown={(event) => {
                  event.preventDefault()
                }}
                onMouseEnter={() => {
                  onHover(index)
                }}
                onClick={() => {
                  if (!stale) onPick(index)
                }}
                className={cn(
                  "flex min-h-14 w-full cursor-pointer items-center gap-3 px-4 py-1 text-left",
                  active ? "bg-surface-2" : "hover:bg-surface-2",
                )}
              >
                <CardImage
                  summary={row.summary}
                  name={row.name.primary}
                  images={images}
                  alt="redundant"
                  sizes="32px"
                  className="w-8 shrink-0"
                />
                <span className="flex min-w-0 flex-1 flex-col leading-tight">
                  <span lang={row.name.primary.lang} className="truncate text-15 font-semibold">
                    {row.name.primary.text}
                  </span>
                  <span className="truncate text-12 text-text-3">{details.join(" · ")}</span>
                  {(row.name.lowConfidence || row.name.sourceUnchecked) && (
                    <span className="flex flex-wrap gap-x-2 text-11 text-text-3">
                      {row.name.lowConfidence && <span>{t("card.translationProofreading")}</span>}
                      {row.name.sourceUnchecked && <span>{t("card.sourceUnchecked")}</span>}
                    </span>
                  )}
                </span>
              </button>
            )
          })}
        </div>
      )}
    </div>
  )
}
