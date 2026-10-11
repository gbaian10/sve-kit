import { useCallback, useEffect, useMemo } from "react"
import { useTranslation } from "react-i18next"

import {
  useCatalog,
  useImageIndex,
  useSearch,
  useSearchReadiness,
  warmTextResolver,
} from "../../app/snapshot"
import { StatusStrip } from "../../app/StatusStrip"
import { rowLabel } from "../../components/card/vocabulary"
import type { SuggestRow } from "../../components/search/SuggestList"
import { type CardSummary, type LoadedSnapshot, type SearchQuery, stringValue } from "../../data"
import { displayName, UI_TEXT_LANG } from "../../domain/nameDisplay"
import { DEFAULT_QUERY } from "../../domain/query/model"
import { packCounts, type ProductFacts, recentSets } from "../../domain/sets"
import { currentUiLanguage } from "../../i18n"
import { usePrefs, useRecent } from "../../settings"
import { RECENT_SET_LIMIT, type SetEntry } from "./home-model"
import { HomeSearch } from "./HomeSearch"
import { NewPackBanner, type PackCountState } from "./NewPackBanner"
import { OfficialNews } from "./OfficialNews"
import { RecentCards } from "./RecentCards"
import { SetEntries } from "./SetEntries"

// Products ship whole in the bootstrap, but the card index has no list of them yet.
function productFacts(snapshot: LoadedSnapshot): ProductFacts[] {
  return snapshot.bootstrap
    .filter((fragment) => fragment.table === "product")
    .flatMap((fragment) => fragment.rows)
    .map((row) => ({
      familyId: typeof row["family_id"] === "string" ? row["family_id"] : null,
      region: stringValue(row["region"]),
      releasedOn:
        row["date_precision"] === "day" && typeof row["released_on"] === "string"
          ? row["released_on"]
          : null,
    }))
}

// The home page (design 00 R1, #519). The shell and the search paint at once and stay usable while
// the catalog downloads; every block that needs data waits for the complete catalog instead of
// showing partial numbers. Phones and the 820 tablet stack the blocks; desktop uses three columns.
export function HomePage() {
  const { t } = useTranslation()
  const prefs = usePrefs()
  const edition = prefs.cardEdition
  const uiLanguage = currentUiLanguage(prefs.uiLanguage)
  const textLang = UI_TEXT_LANG[uiLanguage]
  const { client, status, catalog } = useCatalog()
  const readiness = useSearchReadiness(edition)
  const images = useImageIndex(client, catalog !== null)
  const snapshot = client.snapshot()
  const recent = useRecent()
  const nameOf = useCallback(
    (summary: CardSummary) => displayName(summary.name, uiLanguage, prefs.nameDisplay),
    [uiLanguage, prefs.nameDisplay],
  )
  const recentRows = useMemo<SuggestRow[] | null>(
    () =>
      catalog
        ? recent.flatMap((printingId) => {
            const summary = catalog.summary(printingId)
            return summary ? [{ summary, name: nameOf(summary) }] : []
          })
        : null,
    [catalog, recent, nameOf],
  )
  // Most visits leave the home page for a card: build the text indexes while idle, as /cards does.
  useEffect(() => {
    const first = catalog?.entries[0]
    const printingId = first?.defaultPrinting[edition] ?? first?.printings[0]?.id
    const summary = printingId === undefined ? undefined : catalog?.summary(printingId)
    const revision = summary && catalog?.index.displayRevision(summary.faceId, summary.region)
    if (!revision) return
    return warmTextResolver(
      client,
      { kind: "face_revision", id: stringValue(revision["id"]) },
      "name",
      textLang,
    )
  }, [client, catalog, edition, textLang])
  const products = useMemo(() => (snapshot ? productFacts(snapshot) : null), [snapshot])
  const sets = useMemo<SetEntry[] | null>(() => {
    if (!catalog || !products) return null
    const families = catalog.index.families
    return recentSets(
      families.map((row) => ({
        id: stringValue(row["id"]),
        publicCode: stringValue(row["public_code"]),
        kind: stringValue(row["kind"]),
      })),
      products,
      edition,
      RECENT_SET_LIMIT,
    ).flatMap((set) => {
      const row = catalog.index.family(set.id)
      return row
        ? [
            {
              code: stringValue(row["code"]),
              publicCode: set.publicCode,
              name: rowLabel(catalog.index, row, "name_unit_id", textLang),
              releasedOn: set.releasedOn,
            },
          ]
        : []
    })
  }, [catalog, products, edition, textLang])
  const pack = sets?.[0]
  const packCode = pack?.code
  const packQuery = useMemo<SearchQuery | null>(
    () =>
      packCode === undefined
        ? null
        : { state: { ...DEFAULT_QUERY, sets: [packCode] }, edition, offset: 0 },
    [packCode, edition],
  )
  const packSearch = useSearch(packQuery)
  const counts = useMemo<PackCountState>(() => {
    const { lastInput, result } = packSearch
    if (result.phase === "error") return { phase: "error" }
    if (result.phase === "pending" || lastInput !== packQuery) return { phase: "pending" }
    return {
      phase: "complete",
      counts: packCounts(
        result.page.items.map((item) => ({
          key: item.key,
          translated: Boolean(item.summary?.name.translations["zh-Hant"]),
          lowConfidence: item.summary?.name.translationQuality?.["zh-Hant"]?.lowConfidence ?? false,
        })),
      ),
    }
  }, [packSearch, packQuery])

  return (
    <div className="flex flex-col gap-4 pt-3 pb-8 lg:pt-8">
      <h1 className="sr-only">{t("pages.home")}</h1>
      <HomeSearch
        catalog={catalog}
        failed={status.state === "error"}
        edition={edition}
        images={images}
        recent={recentRows}
        nameOf={nameOf}
      />
      <div className="mx-auto w-full max-w-150 empty:hidden">
        <StatusStrip readiness={readiness} />
      </div>
      <div className="mt-2 flex flex-col gap-6 lg:mx-auto lg:grid lg:w-full lg:max-w-290 lg:grid-cols-3 lg:items-start lg:gap-x-6">
        <div className="flex min-w-0 flex-col gap-6 lg:col-start-2 lg:row-start-1">
          <RecentCards rows={recentRows} stored={recent.length} images={images} />
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
