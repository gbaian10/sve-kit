import {
  type KeyboardEvent,
  type ReactNode,
  type RefObject,
  useCallback,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react"
import { useTranslation } from "react-i18next"

import type { SearchReadiness } from "../../app/snapshot"
import { usePrintingOptions, useSetOptions } from "../../app/snapshot"
import { StatusStrip } from "../../app/StatusStrip"
import { CardGrid, type CardGridProps, type GridCell } from "../../components/results/CardGrid"
import { CardList, CardTable } from "../../components/results/CardRows"
import { ListColumnPicker } from "../../components/results/ListColumnPicker"
import { PaginationTail } from "../../components/results/PaginationTail"
import { SkeletonGrid } from "../../components/results/SkeletonGrid"
import { AbilityTypeFilter } from "../../components/search/AbilityTypeFilter"
import { ArtistFilter } from "../../components/search/ArtistFilter"
import {
  ClassQuickBar,
  ClassQuickBarSkeleton,
  type QuickBarClass,
} from "../../components/search/ClassQuickBar"
import { ConditionChips } from "../../components/search/ConditionChips"
import { vocabularyOptionLabel } from "../../components/search/facet-labels"
import { FilterPanel } from "../../components/search/FilterPanel"
import { KeywordFilter } from "../../components/search/KeywordFilter"
import { LocationFilter } from "../../components/search/LocationFilter"
import { PrintedMechanismFilter } from "../../components/search/PrintedMechanismFilter"
import { RarityFilter } from "../../components/search/RarityFilter"
import { SearchBar } from "../../components/search/SearchBar"
import { SetFilter } from "../../components/search/SetFilter"
import { TraitFilter } from "../../components/search/TraitFilter"
import { TypeFilter } from "../../components/search/TypeFilter"
import { Button } from "../../components/ui/Button"
import { cn } from "../../components/ui/cn"
import { Notice } from "../../components/ui/Notice"
import { Segmented } from "../../components/ui/Segmented"
import type { ImageIndex, ImageSource, QueryFailure, SearchPage } from "../../data"
import { CREDITED_ARTIST, UNCREDITED_ARTIST } from "../../domain/artistFilter"
import { quickBarClasses } from "../../domain/classes"
import { leaderFrameTokens, tokenKindTokens } from "../../domain/deckLocation"
import { formatQuery } from "../../domain/query/codec"
import {
  activeFilterCount,
  DEFAULT_QUERY,
  defaultSecondarySort,
  NEUTRAL_CLASS,
  type QueryState,
  toggleClass,
  VIEW_MODES,
} from "../../domain/query/model"
import { prStyleTokens } from "../../domain/rarityFilter"
import type { Region, TextLang } from "../../domain/search"
import { setCaption } from "../../domain/sets"
import type { UiLanguage } from "../../i18n"
import { usePrefs } from "../../settings"
import { FilterSheet } from "./FilterSheet"
import { FilterSidebar } from "./FilterSidebar"
import { type SidebarEdge, sidebarGridClass } from "./sidebar-layout"
import { SortControls } from "./SortControls"

type ViewMode = (typeof VIEW_MODES)[number]

// Clip only the gutter backdrop, so suggestions can extend below the sticky bar.
const FULL_BLEED =
  "isolate before:pointer-events-none before:absolute before:inset-0 before:-z-10 before:bg-bg before:shadow-[0_0_0_100vmax_var(--color-bg)] before:[clip-path:inset(0_-100vmax)]"

interface CardBrowserSearch {
  readonly value: string
  readonly onChange: (value: string) => void
  readonly onKeyDown: (event: KeyboardEvent<HTMLInputElement>) => void
  readonly onFocus: () => void
  readonly onBlur: () => void
  readonly inputRef: RefObject<HTMLInputElement | null>
  readonly listId: string
  readonly expanded: boolean
  readonly activeOptionId?: string | undefined
  /** Under the search bar inside the sticky header: the suggestion list, a pending note. */
  readonly below?: ReactNode
}

interface CardBrowserResults {
  readonly page: SearchPage | null
  readonly cells: readonly GridCell[]
  readonly images: ImageIndex | undefined
  readonly error: QueryFailure | undefined
  readonly progress: { readonly done: number; readonly total: number } | undefined
  readonly loading: boolean
  readonly pages: number
  readonly onMore: () => void
  readonly onRetry: () => void
}

export interface CardBrowserProps {
  /** The root the image hooks observe; `useListView` provides it. */
  readonly imageRoot: RefObject<HTMLDivElement | null>
  readonly readiness: SearchReadiness
  readonly edition: Region
  readonly uiLanguage: UiLanguage
  readonly textLang: TextLang
  readonly query: QueryState
  readonly onQueryChange: (next: QueryState) => void
  readonly onEditionChange: (edition: Region) => void
  /** "Clear all" buttons; the page decides what else resets (typed text, scope). */
  readonly onClearAll: () => void
  readonly search: CardBrowserSearch
  readonly results: CardBrowserResults
  /** The result count stays through a new query of the same dataset and unit. */
  readonly countScope: string
  readonly view: ViewMode
  readonly onViewChange: (view: ViewMode) => void
  readonly onOpen: CardGridProps["onOpen"]
  readonly anchor?: string | undefined
  readonly selected?: string | undefined
  readonly listScroll: RefObject<{ readonly top: number; readonly left: number }>
  readonly sidebar: {
    /** From the sidebar width (1180) the filters are a column; below it a bottom sheet. */
    readonly layout: boolean
    readonly edge: SidebarEdge
    readonly collapsed: boolean
    readonly onToggle: () => void
  }
  readonly sheetOpen: boolean
  readonly onSheetOpenChange: (open: boolean) => void
  /** Left end of the toolbar row: the deck builder's scope tabs and legal-only switch. */
  readonly toolbarStart?: ReactNode
  /**
   * Whether the result unit ("one row per card") is the user's to choose, with its chip and its
   * count in the filter badge. The deck builder's pool is always per card, so it passes false.
   */
  readonly unitChoice?: boolean
  /** A control per result (the deck builder's stepper); the cards page has none. */
  readonly renderAction?: CardGridProps["renderAction"]
  /** Badges over a grid cell's image (copies in the deck). */
  readonly renderOverlay?: CardGridProps["renderOverlay"]
  /** Rendered inside the root after the results (the card overlay's drawer and modal). */
  readonly children?: ReactNode
}

/**
 * The card pool both pages share: search bar, toolbar (view switch and count at the right end),
 * condition chips, filters (column or sheet), the three views and paging. Pages own the query
 * state, the search results, suggestions and what opens on click; the deck builder adds its
 * scope tabs (`toolbarStart`) and the per-card stepper (`renderAction`, `renderOverlay`).
 */
export function CardBrowser({
  imageRoot,
  readiness,
  edition,
  uiLanguage,
  textLang,
  query,
  onQueryChange,
  onEditionChange,
  onClearAll,
  search,
  results,
  countScope,
  view,
  onViewChange,
  onOpen,
  anchor,
  selected,
  listScroll,
  sidebar,
  sheetOpen,
  onSheetOpenChange,
  toolbarStart,
  unitChoice = true,
  renderAction,
  renderOverlay,
  children,
}: CardBrowserProps) {
  const { t } = useTranslation()
  const prefs = usePrefs()
  const { sets, quickSets, setCounts, rarities, classes } = useSetOptions()
  const printingOptions = usePrintingOptions()
  const panelId = useId()
  const sidebarId = useId()
  const { page, cells, images, error: queryError, progress, loading: pageLoading, pages } = results

  const quickBar: QuickBarClass[] = useMemo(
    () =>
      quickBarClasses(classes?.map((item) => item.code) ?? []).map((code) => {
        const option = classes?.find((item) => item.code === code)
        return {
          code,
          label:
            code === NEUTRAL_CLASS
              ? t("search.neutral")
              : (option?.name.translations[textLang] ?? option?.name.original.text ?? code),
        }
      }),
    [classes, textLang, t],
  )
  const classLabel = (code: string | null) =>
    code === null
      ? t("search.neutral")
      : (quickBar.find((item) => item.code === code)?.label ?? code)
  const typeLabel = (code: string) => {
    const option = printingOptions?.type.types.find((item) => item.code === code)
    return option ? vocabularyOptionLabel(option.name, textLang) : code
  }
  const commit = onQueryChange
  // With the unit chip off, the unit is neither counted nor shown, and chips never change it.
  const chipQuery = unitChoice ? query : { ...query, unit: "printing" as const }
  const filterCount = activeFilterCount(chipQuery)
  const hasConditions = query.text !== "" || filterCount > 0
  // Alt-art is a legacy URL flag without a condition chip.
  const hasVisibleConditions = query.text !== "" || filterCount > (query.altArtOnly ? 1 : 0)

  // Keep the count through a new query, but never carry it into another dataset or result unit.
  const [lastCount, setLastCount] = useState({ scope: countScope, total: page?.total ?? null })
  if (lastCount.scope !== countScope || (page !== null && lastCount.total !== page.total)) {
    setLastCount({ scope: countScope, total: page?.total ?? null })
  }
  const total =
    page?.total ?? (lastCount.scope === countScope && !queryError ? lastCount.total : null)
  const unsupported = queryError?.kind === "unsupported-query"
  const pending = !queryError && page === null
  // While a changed query waits for its results, the skeleton keeps the previous results' height,
  // so the page never shrinks under the user's scroll position.
  const resultsRef = useRef<HTMLDivElement>(null)
  const resultsHeight = useRef(0)
  useLayoutEffect(() => {
    const node = resultsRef.current
    if (!node) return
    if (pending) node.style.minHeight = `${String(resultsHeight.current)}px`
    else {
      node.style.minHeight = ""
      resultsHeight.current = node.offsetHeight
    }
  }, [pending, page])
  const artOf = useCallback(
    (cell: GridCell): ImageSource | undefined =>
      images?.artImage?.(cell.summary.printingId, cell.summary.faceId),
    [images],
  )

  // The count sits at the right end of the toolbar, beside the view switch, with its width
  // reserved so a pending query does not shift the controls around it (round 6).
  const resultCount = (
    <div
      className="min-w-20 shrink-0 text-right text-13 whitespace-nowrap text-text-2"
      aria-live="polite"
      aria-busy={page === null}
    >
      {total !== null ? (
        <>
          <p>
            <b className="font-semibold text-text-1 tabular-nums">
              {new Intl.NumberFormat(uiLanguage).format(total)}
            </b>{" "}
            <span>
              {query.unit === "printing"
                ? t("results.countPrintingUnit")
                : query.unit === "art"
                  ? t("results.countArtUnit")
                  : t("results.countUnit")}
            </span>
          </p>
          {Object.keys(query.mechanics).length > 0 &&
            printingOptions?.keyword.coverage === "partial" && (
              <p className="text-11">
                {printingOptions.keyword.unknownCards !== undefined
                  ? t("filters.keywordUnknownCount", {
                      count: printingOptions.keyword.unknownCards,
                    })
                  : t("filters.keywordIncomplete")}
              </p>
            )}
        </>
      ) : (
        <p aria-hidden="true">…</p>
      )}
    </div>
  )

  // The view only changes presentation, so it does not restart automatic paging.
  const { view: _view, ...conditions } = query
  const pagination =
    page && page.total > 0 ? (
      <PaginationTail
        unit={query.unit}
        automatic={prefs.infiniteScroll}
        requestKey={JSON.stringify([countScope, formatQuery(conditions).toString(), pages])}
        total={page.total}
        hasMore={page.items.length < page.total}
        loading={
          pageLoading || (!queryError && page.items.length < Math.min(pages * 60, page.total))
        }
        failed={Boolean(queryError)}
        onMore={results.onMore}
        onRetry={results.onRetry}
      />
    ) : null

  const classBar = (
    <>
      {classes === undefined && <ClassQuickBarSkeleton />}
      {classes !== undefined && quickBar.length > 0 && (
        <ClassQuickBar
          label={t("search.classes")}
          classes={quickBar}
          selected={query.classes}
          onToggle={(code) => {
            commit(toggleClass(query, code))
          }}
        />
      )}
    </>
  )
  const setsBar = (
    <SetFilter
      sets={sets}
      quickSets={quickSets}
      counts={setCounts}
      textLang={textLang}
      selected={query.sets}
      onChange={(selected) => {
        commit({ ...query, sets: selected })
      }}
    />
  )
  const panelEl = (
    <FilterPanel
      sidebar={sidebar.layout}
      unitChoice={unitChoice}
      id={panelId}
      open={sheetOpen}
      query={query}
      onChange={commit}
      edition={edition}
      onEditionChange={onEditionChange}
      sortControls={<SortControls query={query} onChange={commit} />}
      restrictions={printingOptions?.restrictions}
      resultTotal={page?.total}
      onClose={() => {
        onSheetOpenChange(false)
        requestAnimationFrame(() => {
          resultsRef.current?.scrollIntoView({ block: "start" })
        })
      }}
      types={
        <TypeFilter
          options={printingOptions?.type}
          textLang={textLang}
          selected={query.types}
          onChange={(types) => {
            commit({ ...query, types })
          }}
        />
      }
      locations={
        <LocationFilter
          options={printingOptions?.location}
          textLang={textLang}
          selected={query.locations}
          onChange={(locations) => {
            commit({ ...query, locations })
          }}
        />
      }
      facets={{
        rarity: (
          <RarityFilter
            rarities={(rarities ?? []).map((rarity) => ({
              code: rarity.code,
              label: rarity.name.translations[textLang] ?? rarity.name.original.text,
            }))}
            options={printingOptions?.rarity}
            edition={edition}
            selected={query.rarities}
            onChange={(value) => {
              commit({ ...query, rarities: value })
            }}
          />
        ),
        traits: (
          <TraitFilter
            options={printingOptions?.trait}
            textLang={textLang}
            selected={query.traits}
            onChange={(traits) => {
              commit({ ...query, traits })
            }}
          />
        ),
        artists: (
          <ArtistFilter
            options={printingOptions?.artist}
            edition={edition}
            selected={query.artists}
            onChange={(artists) => {
              commit({ ...query, artists })
            }}
          />
        ),
        abilityTypes: (
          <AbilityTypeFilter
            options={printingOptions?.abilityTypes}
            selected={query.abilityTypes}
            matchMode={query.abilityTypesMatch}
            onChange={(abilityTypes) => {
              commit({ ...query, abilityTypes })
            }}
            onMatchChange={(abilityTypesMatch) => {
              commit({ ...query, abilityTypesMatch })
            }}
          />
        ),
        classMechanisms: (
          <PrintedMechanismFilter
            group="classMechanisms"
            options={printingOptions?.classMechanisms}
            selected={query.classMechanisms}
            matchMode={query.classMechanismsMatch}
            onChange={(classMechanisms) => {
              commit({ ...query, classMechanisms })
            }}
            onMatchChange={(classMechanismsMatch) => {
              commit({ ...query, classMechanismsMatch })
            }}
          />
        ),
        collabMechanisms: (
          <PrintedMechanismFilter
            group="collabMechanisms"
            options={printingOptions?.collabMechanisms}
            selected={query.collabMechanisms}
            matchMode={query.collabMechanismsMatch}
            onChange={(collabMechanisms) => {
              commit({ ...query, collabMechanisms })
            }}
            onMatchChange={(collabMechanismsMatch) => {
              commit({ ...query, collabMechanismsMatch })
            }}
          />
        ),
        keywords: (
          <KeywordFilter
            options={printingOptions?.keyword}
            textLang={textLang}
            edition={edition}
            selected={query.mechanics}
            matchMode={query.mechanicsMatch}
            onMatchChange={(mechanicsMatch) => {
              commit({ ...query, mechanicsMatch })
            }}
            onChange={(mechanics) => {
              commit({ ...query, mechanics })
            }}
          />
        ),
      }}
    />
  )
  const conditionChips = (
    <ConditionChips
      query={chipQuery}
      edition={edition}
      classLabel={classLabel}
      rarityLabel={(code) => {
        const option = rarities?.find((item) => item.code === code)
        return option
          ? (option.name.translations[textLang] ?? option.name.original.text)
          : code.toUpperCase()
      }}
      typeLabel={typeLabel}
      traitLabel={(code) => {
        const option = printingOptions?.trait.traits.find((item) => item.code === code)
        return option ? vocabularyOptionLabel(option.name, textLang) : code
      }}
      artistLabel={(id) => {
        if (id === CREDITED_ARTIST) return t("filters.artistCredited")
        if (id === UNCREDITED_ARTIST) return t("filters.artistUncredited")
        const option = printingOptions?.artist.artists.find((item) => item.id === id)
        return option ? option.name : id
      }}
      keywordLabel={(id) => {
        const option = printingOptions?.keyword.keywords.find((item) => item.id === id)
        return option ? vocabularyOptionLabel(option.name, textLang) : id
      }}
      setLabel={(code) => {
        const option = sets?.find((item) => item.code === code)
        return option ? setCaption(option, textLang).text : code
      }}
      groups={{
        pr: prStyleTokens(printingOptions?.rarity.styles ?? []),
        token: tokenKindTokens(printingOptions?.location.counts ?? {}),
        leader: leaderFrameTokens(printingOptions?.location.counts ?? {}),
      }}
      onChange={(next) => {
        commit(unitChoice ? next : { ...next, unit: query.unit })
      }}
      onClear={onClearAll}
    />
  )
  const viewSwitch = (
    <Segmented
      size="sm"
      tone="accent"
      label={t("results.view")}
      options={VIEW_MODES.map((value) => ({ value, label: t(`options.view.${value}`) }))}
      value={view}
      onChange={onViewChange}
    />
  )
  const viewProps = {
    cells,
    onOpen,
    ...(anchor === undefined ? {} : { anchor }),
    ...(selected === undefined ? {} : { selected }),
    ...(renderAction ? { renderAction } : {}),
  }
  return (
    <div ref={imageRoot} className="flex flex-col gap-3 pt-2 pb-6">
      <StatusStrip readiness={readiness} />
      {query.restrictionsOnly && (
        <Notice tone="info" role="status">
          {!printingOptions?.restrictions || printingOptions.restrictions.state === "unknown"
            ? t("filters.restrictionsUnknown")
            : t(
                printingOptions.restrictions.synthetic
                  ? "filters.restrictionsSyntheticDate"
                  : "filters.restrictionsDate",
                { date: printingOptions.restrictions.onDate },
              )}
          {printingOptions?.restrictions?.synthetic ? ` ${t("filters.restrictionsSynthetic")}` : ""}
          {printingOptions?.restrictions?.state === "partial"
            ? ` ${t("filters.restrictionsPartial")}`
            : ""}
        </Notice>
      )}
      <div
        className={
          sidebar.layout
            ? cn(
                "grid min-h-[calc(100dvh-3.25rem)] items-start gap-6",
                sidebarGridClass(sidebar.edge, sidebar.collapsed),
              )
            : "flex flex-col gap-3"
        }
      >
        {sidebar.layout && (
          <FilterSidebar
            id={sidebarId}
            edge={sidebar.edge}
            collapsed={sidebar.collapsed}
            onToggle={sidebar.onToggle}
            filterCount={filterCount}
          >
            {classBar}
            {setsBar}
            {panelEl}
          </FilterSidebar>
        )}
        <div className="flex min-w-0 flex-col gap-3">
          <div
            data-results-header=""
            className={`@container sticky top-13 z-30 flex flex-col gap-2 py-2 ${FULL_BLEED}`}
          >
            <div className="relative z-20">
              <SearchBar
                value={search.value}
                onChange={search.onChange}
                onKeyDown={search.onKeyDown}
                onFocus={search.onFocus}
                onBlur={search.onBlur}
                inputRef={search.inputRef}
                listId={search.listId}
                expanded={search.expanded}
                {...(search.activeOptionId === undefined
                  ? {}
                  : { activeOptionId: search.activeOptionId })}
                filterCount={filterCount}
                filtersOpen={sidebar.layout ? !sidebar.collapsed : sheetOpen}
                filtersId={sidebar.layout ? sidebarId : panelId}
                onToggleFilters={() => {
                  if (sidebar.layout) sidebar.onToggle()
                  else onSheetOpenChange(!sheetOpen)
                }}
              />
              {search.below}
            </div>
            <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-2">
              {toolbarStart}
              {total !== null && (
                <div className="ml-auto flex shrink-0 items-center gap-3">
                  {viewSwitch}
                  {resultCount}
                </div>
              )}
            </div>
            {hasVisibleConditions && (
              // Chip hit areas extend vertically; reserve room so the row has no vertical overflow.
              <div
                data-condition-scroll=""
                className="min-w-0 overflow-x-auto py-2 [&>[role=group]]:flex-nowrap [&>[role=group]>*]:shrink-0"
              >
                {conditionChips}
              </div>
            )}
          </div>
          {!sidebar.layout && (
            <FilterSheet
              open={sheetOpen}
              onClose={() => {
                onSheetOpenChange(false)
              }}
            >
              {classBar}
              {setsBar}
              {panelEl}
            </FilterSheet>
          )}
          {unsupported && (
            <Notice
              tone="warning"
              role="alert"
              actions={
                <Button
                  className="h-9"
                  onClick={() => {
                    commit({ ...DEFAULT_QUERY, text: query.text, classes: query.classes })
                  }}
                >
                  {t("results.clearAll")}
                </Button>
              }
            >
              {t("results.unsupported", {
                facets: (queryError.facets ?? [])
                  .map((facet) => t(`results.facet.${facet}`))
                  .join("、"),
              })}
            </Notice>
          )}
          {queryError && !unsupported && page === null && readiness.phase !== "error" && (
            <Notice
              tone="danger"
              role="alert"
              actions={<Button onClick={results.onRetry}>{t("results.retry")}</Button>}
            >
              {t("results.loadFailed")}
            </Notice>
          )}
          <div ref={resultsRef} data-results="" className="flex min-w-0 flex-col gap-3">
            {/* The status strip above says what is downloading; the list only shows its skeleton. */}
            {pending && readiness.phase !== "error" && (
              <>
                {readiness.phase === "ready" && (
                  <p role="status" className="text-13 text-text-2">
                    {progress
                      ? t("results.searchProgress", progress)
                      : t("results.dependenciesPending")}
                  </p>
                )}
                <SkeletonGrid />
              </>
            )}
            {page !== null && page.total === 0 && (
              <div className="flex flex-col items-center gap-3 rounded-block border border-dashed border-border-strong px-4 py-10 text-center">
                <p className="text-16 font-semibold">{t("results.empty")}</p>
                <p className="text-13 text-text-3">{t("results.emptyHint")}</p>
                {hasConditions && <Button onClick={onClearAll}>{t("results.clearAll")}</Button>}
              </div>
            )}
            {page !== null && page.total > 0 && (
              <>
                {view === "list" && <ListColumnPicker />}
                {view === "grid" && (
                  <CardGrid
                    images={images}
                    {...viewProps}
                    {...(renderOverlay ? { renderOverlay } : {})}
                  />
                )}
                {view === "table" && (
                  <CardTable typeLabel={typeLabel} images={images} artOf={artOf} {...viewProps} />
                )}
                {view === "list" && (
                  <CardList
                    classLabel={classLabel}
                    typeLabel={typeLabel}
                    footer={pagination}
                    sort={query.sort}
                    descending={query.order === "desc"}
                    onSort={(sort) => {
                      const { order, ...rest } = query
                      const next =
                        sort === query.sort && order !== "desc"
                          ? { ...rest, sort, order: "desc" as const }
                          : { ...rest, sort }
                      if (next.secondarySort === sort) {
                        const { secondaryOrder: _secondaryOrder, ...distinct } = next
                        commit({ ...distinct, secondarySort: defaultSecondarySort(sort) })
                      } else commit(next)
                    }}
                    scrollPositionRef={listScroll}
                    {...viewProps}
                  />
                )}
                {view !== "list" && pagination}
              </>
            )}
          </div>
        </div>
      </div>
      {children}
    </div>
  )
}
