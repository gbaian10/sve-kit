import { useCallback, useMemo } from "react"
import { useTranslation } from "react-i18next"

import {
  useRecentSummaries,
  useSearchReadiness,
  useSetCounts,
  useSetReleases,
} from "../../app/snapshot"
import { StatusStrip } from "../../app/StatusStrip"
import type { SuggestRow } from "../../components/search/SuggestList"
import type { CardSummary } from "../../data"
import { displayName, UI_TEXT_LANG } from "../../domain/nameDisplay"
import { recentSets } from "../../domain/sets"
import { currentUiLanguage } from "../../i18n"
import { usePrefs, useRecent } from "../../settings"
import { faceImages, RECENT_SET_LIMIT, type SetEntry, setLabel } from "./home-model"
import { HomeSearch } from "./HomeSearch"
import { NewPackBanner, type PackCountState } from "./NewPackBanner"
import { OfficialNews } from "./OfficialNews"
import { RecentCards } from "./RecentCards"
import { SetEntries } from "./SetEntries"

// Everything here comes from bounded Worker answers; the home page never loads the full snapshot.
export function HomePage() {
  const { t } = useTranslation()
  const prefs = usePrefs()
  const edition = prefs.cardEdition
  const uiLanguage = currentUiLanguage(prefs.uiLanguage)
  const textLang = UI_TEXT_LANG[uiLanguage]
  const readiness = useSearchReadiness(edition)
  const ready =
    readiness.phase === "ready" && readiness.searchReady !== false && readiness.edition === edition
  const failed = readiness.phase === "error"
  const recent = useRecent()
  const nameOf = useCallback(
    (summary: CardSummary) => displayName(summary.name, uiLanguage, prefs.nameDisplay),
    [uiLanguage, prefs.nameDisplay],
  )
  const recentResult = useRecentSummaries(recent, edition)
  const recentRows = useMemo<SuggestRow[] | null>(
    () =>
      recentResult.phase === "complete"
        ? (recentResult.recent ?? []).flatMap((item) =>
            item.summary ? [{ summary: item.summary, name: nameOf(item.summary) }] : [],
          )
        : null,
    [recentResult, nameOf],
  )
  const recentImages = useMemo(
    () =>
      recentResult.phase === "complete" && recentResult.faces
        ? faceImages(recentResult.faces, edition)
        : undefined,
    [recentResult, edition],
  )
  const releases = useSetReleases(edition)
  const sets = useMemo<SetEntry[] | null>(() => {
    const { lastInput, result } = releases
    if (result.phase !== "complete" || !result.releases || lastInput?.edition !== edition)
      return null
    return recentSets(result.releases, RECENT_SET_LIMIT).map((set) => ({
      code: set.code,
      publicCode: set.publicCode,
      name: setLabel(set.name, textLang),
      releasedOn: set.releasedOn,
    }))
  }, [releases, edition, textLang])
  const pack = sets?.[0]
  const packCode = pack?.code
  const packCount = useSetCounts(packCode, edition, "zh-Hant")
  const counts = useMemo<PackCountState>(() => {
    const { lastInput, result } = packCount
    if (result.phase === "error") return { phase: "error" }
    if (
      result.phase !== "complete" ||
      !result.counts ||
      lastInput === null ||
      lastInput.state.sets[0] !== packCode ||
      lastInput.edition !== edition
    )
      return { phase: "pending" }
    return { phase: "complete", counts: result.counts }
  }, [packCount, packCode, edition])

  return (
    <div className="flex flex-col gap-4 pt-3 pb-8 lg:pt-8">
      <h1 className="sr-only">{t("pages.home")}</h1>
      <HomeSearch
        ready={ready}
        failed={failed}
        edition={edition}
        recent={recentRows}
        recentImages={recentImages}
        nameOf={nameOf}
      />
      <div className="mx-auto w-full max-w-150 empty:hidden">
        <StatusStrip readiness={readiness} />
      </div>
      <div className="mt-2 flex flex-col gap-6 lg:mx-auto lg:grid lg:w-full lg:max-w-290 lg:grid-cols-3 lg:items-start lg:gap-x-6">
        <div className="flex min-w-0 flex-col gap-6 lg:col-start-2 lg:row-start-1">
          <RecentCards rows={recentRows} stored={recent.length} images={recentImages} />
          {pack !== undefined && (
            <NewPackBanner
              pack={pack}
              counts={counts}
              countTranslations={uiLanguage === "zh-TW"}
              locale={uiLanguage}
            />
          )}
        </div>
        <div className="min-w-0 lg:col-start-1 lg:row-start-1">
          <SetEntries sets={sets} />
        </div>
        <div className="min-w-0 lg:col-start-3 lg:row-start-1">
          <OfficialNews edition={edition} />
        </div>
      </div>
    </div>
  )
}
