import { useId } from "react"
import { useTranslation } from "react-i18next"
import { Link as RouterLink } from "react-router"

import type { PackCounts } from "../../domain/sets"
import { type SetEntry, setPath } from "./home-model"

export type PackCountState =
  | { readonly phase: "pending" }
  | { readonly phase: "error" }
  | { readonly phase: "complete"; readonly counts: PackCounts }

export interface NewPackBannerProps {
  readonly pack: SetEntry
  readonly counts: PackCountState
  /** Traditional Chinese names are only counted on the Traditional Chinese interface. */
  readonly countTranslations: boolean
  readonly locale: string
}

export function NewPackBanner({ pack, counts, countTranslations, locale }: NewPackBannerProps) {
  const { t } = useTranslation()
  const headingId = useId()
  const number = new Intl.NumberFormat(locale)
  return (
    <section
      aria-labelledby={headingId}
      className="flex flex-col gap-3 rounded-block border border-border bg-surface-1 p-4"
    >
      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
          <p className="text-12 text-text-3">
            {t("home.pack.label")}
            {t("cardPage.separator")}
            <span className="tabular-nums">
              {t("home.sets.released", { date: pack.releasedOn })}
            </span>
          </p>
          <h2 id={headingId} lang={pack.name.lang} className="text-16 font-bold">
            {pack.name.text}
          </h2>
        </div>
        <RouterLink
          to={setPath(pack.publicCode)}
          className="flex h-9 shrink-0 items-center rounded-control border border-border-strong px-3 text-13 font-semibold text-text-1 hover:bg-surface-2"
        >
          {t("home.pack.open")}
        </RouterLink>
      </div>
      {counts.phase === "complete" ? (
        <p className="flex flex-wrap gap-x-4 gap-y-1 text-13 text-text-2 tabular-nums">
          <span>{t("home.pack.cards", { value: number.format(counts.counts.cards) })}</span>
          {countTranslations && (
            <span>
              {t("home.pack.translated", { value: number.format(counts.counts.translated) })}
            </span>
          )}
          {countTranslations && counts.counts.proofreading > 0 && (
            <span>
              {t("home.pack.proofreading", {
                value: number.format(counts.counts.proofreading),
              })}
            </span>
          )}
        </p>
      ) : (
        <p role="status" className="text-13 text-text-3">
          {counts.phase === "pending"
            ? t("home.pack.countsPending")
            : t("home.pack.countsUnavailable")}
        </p>
      )}
    </section>
  )
}
