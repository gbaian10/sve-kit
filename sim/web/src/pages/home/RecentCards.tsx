import { type MouseEvent, useId } from "react"
import { useTranslation } from "react-i18next"
import { useHref, useNavigate } from "react-router"

import { type CardEntryState, routeState } from "../../app/listEntryState"
import { CardImage } from "../../components/card/CardImage"
import { isPlainClick } from "../../components/results/plain-click"
import type { SuggestRow } from "../../components/search/SuggestList"
import { Button } from "../../components/ui/Button"
import type { ImageIndex } from "../../data"
import { recentStore } from "../../settings"
import { cardPath } from "./home-model"

export interface RecentCardsProps {
  /** Rows of the ids the catalog knows; null while the catalog is not loaded. */
  readonly rows: readonly SuggestRow[] | null
  /** How many ids are stored, so a loading list keeps its height. */
  readonly stored: number
  readonly images: ImageIndex | undefined
  readonly failed?: boolean
  readonly onRetry?: () => void
}

// Unknown ids may belong to another snapshot, so resolving the list never deletes them.
export function RecentCards({ rows, stored, images, failed = false, onRetry }: RecentCardsProps) {
  const { t } = useTranslation()
  const headingId = useId()
  const count = rows === null ? stored : rows.length
  return (
    <section aria-labelledby={headingId} className="flex flex-col gap-2">
      <div className="flex min-h-8 items-center justify-between gap-3">
        <h2 id={headingId} className="text-15 font-semibold">
          {t("search.recent")}
        </h2>
        {(rows !== null || failed) && stored > 0 && (
          <button
            type="button"
            onClick={() => {
              recentStore.clear()
            }}
            className="h-8 rounded-control px-2 text-13 font-semibold text-accent-text hover:bg-surface-2"
          >
            {t("search.clearRecent")}
          </button>
        )}
      </div>
      {failed ? (
        <div role="alert" className="text-13 leading-relaxed text-text-2">
          <p>{t("home.recentFailed")}</p>
          {onRetry && (
            <Button className="mt-2 h-9" onClick={onRetry}>
              {t("status.retry")}
            </Button>
          )}
        </div>
      ) : count === 0 ? (
        <p className="text-13 leading-relaxed text-text-3">
          {stored > 0 ? t("home.recentUnavailable") : t("search.noRecent")}
        </p>
      ) : rows === null ? (
        <ul
          aria-busy="true"
          aria-label={t("home.recentLoading")}
          className="grid gap-2 md:grid-cols-2 lg:grid-cols-1"
        >
          {Array.from({ length: stored }, (_, index) => (
            <li
              key={index}
              className="h-14 animate-pulse rounded-button border border-border bg-surface-1"
            />
          ))}
        </ul>
      ) : (
        <ul className="grid gap-2 md:grid-cols-2 lg:grid-cols-1">
          {rows.map((row) => (
            <li key={row.summary.printingId} className="min-w-0">
              <RecentLink row={row} images={images} />
            </li>
          ))}
        </ul>
      )}
    </section>
  )
}

function RecentLink({
  row,
  images,
}: {
  readonly row: SuggestRow
  readonly images: ImageIndex | undefined
}) {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const to = cardPath(row.summary)
  const href = useHref(to)
  const details = [
    row.name.secondary?.text,
    row.name.missingTranslation ? t("card.noTranslation") : undefined,
    row.summary.cardNo,
  ].filter((part): part is string => part !== undefined)
  return (
    <a
      href={href}
      onClick={(event: MouseEvent<HTMLAnchorElement>) => {
        if (!isPlainClick(event)) return
        event.preventDefault()
        recentStore.push(row.summary.printingId)
        const state: CardEntryState = {
          background: "",
          source: "results",
          pages: 1,
          resultKey: row.summary.printingId,
          ...routeState(row.summary),
        }
        void navigate(to, { state })
      }}
      className="flex h-14 items-center gap-3 rounded-button border border-border bg-surface-1 px-3 text-text-1 hover:bg-surface-2"
    >
      <CardImage
        summary={row.summary}
        name={row.name.primary}
        images={images}
        alt="redundant"
        sizes="28px"
        className="w-7 shrink-0"
      />
      <span className="block min-w-0 flex-1 leading-tight">
        <span lang={row.name.primary.lang} className="block truncate text-14 font-semibold">
          {row.name.primary.text}
        </span>
        <span className="block truncate text-12 text-text-3">{details.join(" · ")}</span>
        {row.name.lowConfidence && (
          <span className="block text-11 text-text-3">{t("card.translationProofreading")}</span>
        )}
      </span>
    </a>
  )
}
