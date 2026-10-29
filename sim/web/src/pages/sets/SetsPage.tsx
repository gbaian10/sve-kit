import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import { Link as RouterLink, useNavigate } from "react-router"

import {
  useRouteEdition,
  useSearchReadiness,
  useSetOptions,
  useSetReleases,
} from "../../app/snapshot"
import { StatusStrip } from "../../app/StatusStrip"
import { Segmented } from "../../components/ui/Segmented"
import { UI_TEXT_LANG } from "../../domain/nameDisplay"
import { editionUrl } from "../../domain/query/codec"
import type { Region } from "../../domain/search"
import { groupSets, setCaption, setCardsPath } from "../../domain/sets"
import { currentUiLanguage } from "../../i18n"
import { prefsStore, usePrefs } from "../../settings"
import { PageTitle } from "../PageTitle"

const EDITIONS = ["jp", "en"] as const

// Lists the families that own at least one printing in the edition (the Worker's set options), so
// an edition-only product shows up under its own edition and nowhere else. Release dates and
// printing counts are supplements: a family without a dated product is still listed.
export function SetsPage() {
  const { t } = useTranslation()
  const edition = useRouteEdition()
  const readiness = useSearchReadiness(edition)
  const { sets, setCounts } = useSetOptions()
  const releases = useSetReleases(edition)
  const navigate = useNavigate()
  const { uiLanguage: preferred } = usePrefs()
  const uiLanguage = currentUiLanguage(preferred)
  const textLang = UI_TEXT_LANG[uiLanguage]
  const number = new Intl.NumberFormat(uiLanguage)
  const separator = t("cardPage.separator")
  const releasedOn = useMemo(() => {
    const { lastInput, result } = releases
    const dated = new Map<string, string>()
    if (result.phase === "complete" && lastInput?.edition === edition)
      for (const release of result.releases ?? []) dated.set(release.id, release.releasedOn)
    return dated
  }, [releases, edition])
  const grouped = useMemo(() => (sets ? groupSets(sets) : null), [sets])
  const changeEdition = (next: Region) => {
    if (next === edition) return
    prefsStore.set({ cardEdition: next })
    void navigate(editionUrl("/sets", next))
  }
  const list = (items: readonly NonNullable<typeof sets>[number][]) => (
    <ul className="grid grid-cols-1 gap-2 md:grid-cols-2 lg:grid-cols-3">
      {items.map((set) => {
        const { code, name } = setCaption(set, textLang)
        const released = releasedOn.get(set.id)
        const count = setCounts?.[set.code]
        return (
          <li key={set.id} className="min-w-0">
            <RouterLink
              to={setCardsPath(set.code, edition)}
              className="flex min-h-14 min-w-0 items-center gap-3 rounded-button border border-border bg-surface-1 px-3 py-1.5 text-text-1 hover:bg-surface-2"
            >
              {code !== null && (
                <span className="min-w-14 shrink-0 font-sans text-14 font-bold tabular-nums">
                  {code}
                </span>
              )}
              <span className="block min-w-0 flex-1 leading-tight">
                <span lang={name.lang} className="block truncate text-14">
                  {name.text}
                </span>
                {(released !== undefined || count !== undefined) && (
                  <span className="block text-12 text-text-3 tabular-nums">
                    {released !== undefined && t("home.sets.released", { date: released })}
                    {released !== undefined && count !== undefined && separator}
                    {count !== undefined &&
                      `${number.format(count)} ${t("results.countPrintingUnit")}`}
                  </span>
                )}
              </span>
            </RouterLink>
          </li>
        )
      })}
    </ul>
  )
  return (
    <div className="pb-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <PageTitle>{t("pages.sets")}</PageTitle>
        <Segmented
          size="sm"
          label={t("filters.edition")}
          options={EDITIONS.map((value) => ({ value, label: t(`options.edition.${value}`) }))}
          value={edition}
          onChange={changeEdition}
        />
      </div>
      <StatusStrip readiness={readiness} />
      {grouped === null ? (
        readiness.phase !== "error" && (
          <p role="status" className="mt-3 text-13 text-text-3">
            {t("home.sets.loading")}
          </p>
        )
      ) : grouped.main.size === 0 && grouped.other.length === 0 ? (
        <p className="mt-3 text-13 leading-relaxed text-text-3">{t("sets.empty")}</p>
      ) : (
        <>
          {[...grouped.main].map(([kind, items]) => (
            <section key={kind} className="mt-5">
              <h2 className="mb-3 text-16 font-semibold">{t(`sets.kind.${kind}`)}</h2>
              {list(items)}
            </section>
          ))}
          {grouped.other.length > 0 && (
            <details className="mt-5">
              <summary className="min-h-11 cursor-pointer py-3 text-16 font-semibold">
                {t("sets.specialProducts")}
              </summary>
              {list(grouped.other)}
            </details>
          )}
        </>
      )}
    </div>
  )
}
