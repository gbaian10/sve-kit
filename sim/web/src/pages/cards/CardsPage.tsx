import {
  type KeyboardEvent,
  useCallback,
  useEffect,
  useEffectEvent,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react"
import { useTranslation } from "react-i18next"
import {
  NavigationType,
  useLocation,
  useNavigate,
  useNavigationType,
  useParams,
  useSearchParams,
} from "react-router"

import { type CardEntryState, routeState, useListEntryState } from "../../app/listEntryState"
import {
  useListView,
  useRecentSummaries,
  useRouteEdition,
  useSearch,
  useSearchReadiness,
  useSetOptions,
  useWorkerSuggestions,
} from "../../app/snapshot"
import { StatusStrip } from "../../app/StatusStrip"
import { useDebouncedValue } from "../../app/useDebouncedValue"
import { CardGrid, type GridCell } from "../../components/results/CardGrid"
import { CardList, CardTable } from "../../components/results/CardRows"
import { PaginationTail } from "../../components/results/PaginationTail"
import { SkeletonGrid } from "../../components/results/SkeletonGrid"
import { ClassQuickBar, type QuickBarClass } from "../../components/search/ClassQuickBar"
import { SearchBar } from "../../components/search/SearchBar"
import { optionId } from "../../components/search/suggest-ids"
import { SuggestList, type SuggestRow } from "../../components/search/SuggestList"
import { Button } from "../../components/ui/Button"
import { Chip } from "../../components/ui/Chip"
import { Notice } from "../../components/ui/Notice"
import { Segmented } from "../../components/ui/Segmented"
import { type CardSummary, type ImageSource, type SearchPage, type SearchQuery } from "../../data"
import { printingPath } from "../../domain/cardRoute"
import { quickBarClasses } from "../../domain/classes"
import { displayName, UI_TEXT_LANG } from "../../domain/nameDisplay"
import { formatQuery, parseQuery, querySearch } from "../../domain/query/codec"
import {
  activeFilterCount,
  DEFAULT_QUERY,
  NEUTRAL_CLASS,
  QUERY_SORTS,
  type QueryState,
  toggleClass,
  VIEW_MODES,
} from "../../domain/query/model"
import { currentUiLanguage } from "../../i18n"
import { prefsStore, recentStore, usePrefs, useRecent } from "../../settings"

const DEBOUNCE_MS = 150

function cardPath(summary: CardSummary): string {
  return printingPath(summary)
}

function createCellReader(nameOf: (summary: CardSummary) => GridCell["name"]) {
  const cells = new WeakMap<SearchPage["items"][number], GridCell>()
  return (item: SearchPage["items"][number]): GridCell | undefined => {
    if (!item.summary) return undefined
    const existing = cells.get(item)
    if (existing) return existing
    const cell = {
      key: item.key,
      summary: item.summary,
      name: nameOf(item.summary),
      to: cardPath(item.summary),
      state: null,
    }
    cells.set(item, cell)
    return cell
  }
}

// The URL holds committed conditions; the input draft drives independent Worker suggestions.
export function CardsPage() {
  const { t } = useTranslation()
  const prefs = usePrefs()
  const uiLanguage = currentUiLanguage(prefs.uiLanguage)
  const textLang = UI_TEXT_LANG[uiLanguage]
  const edition = useRouteEdition()
  const readiness = useSearchReadiness(edition)
  const [params, setParams] = useSearchParams()
  const { code } = useParams()
  const { sets } = useSetOptions()
  const set = code ? sets?.find((set) => set.publicCode === code) : undefined
  const query = useMemo(() => {
    const parsed = parseQuery(params)
    return code && !params.has("set") ? { ...parsed, sets: set ? [set.code] : [] } : parsed
  }, [params, set, code])
  const urlParams = (next: QueryState) => {
    const result = formatQuery(next)
    // A set page's own set is implied by its path, so each state keeps one URL.
    if (set && next.sets.length === 1 && next.sets[0] === set.code) result.delete("set")
    if (edition === "en") result.set("ed", "en")
    return result
  }
  const [entry, updateEntry] = useListEntryState()
  const listScroll = useRef(entry.listScroll ?? { top: 0, left: 0 })
  const restoreScroll = useRef(entry.scrollY)
  const navigate = useNavigate()
  const location = useLocation()
  const navigationType = useNavigationType()
  const recent = useRecent()

  // The typed text is a draft on top of one history entry: any navigation (commit, back, forward)
  // changes `location.key`, the draft stops applying and the input shows the URL's text again.
  const [edit, setEdit] = useState<{ readonly key: string; readonly value: string } | null>(null)
  const input = edit !== null && edit.key === location.key ? edit.value : query.text
  const setInput = (value: string) => {
    setEdit({ key: location.key, value })
  }
  const [focused, setFocused] = useState(false)
  // The highlighted option belongs to one debounced text; another text starts from "none".
  const [highlight, setHighlight] = useState<{
    readonly text: string
    readonly index: number
  } | null>(null)
  const armed = useRef<string | null>(null)
  const inputRef = useRef<HTMLInputElement>(null)
  const listId = useId()
  const debounced = useDebouncedValue(input, DEBOUNCE_MS)
  const active =
    focused && highlight !== null && highlight.text === debounced ? highlight.index : -1
  const setActive = (index: number) => {
    setHighlight({ text: debounced, index })
  }

  const commit = (next: QueryState) => {
    updateEntry({ pages, listScroll: listScroll.current, scrollY: window.scrollY })
    setEdit(null)
    setParams(urlParams(next))
  }
  const nameOf = useCallback(
    (summary: CardSummary) => displayName(summary.name, uiLanguage, prefs.nameDisplay),
    [uiLanguage, prefs.nameDisplay],
  )

  const suggestionState = useWorkerSuggestions(debounced, edition)
  const suggestionResult = suggestionState.result
  const suggestionRows =
    suggestionResult.phase === "complete" ? suggestionResult : suggestionState.previous
  const recentResult = useRecentSummaries(recent, edition)
  const { classes } = useSetOptions()
  const typed = input.trim()
  const stale =
    input !== debounced ||
    suggestionResult.phase !== "complete" ||
    suggestionState.lastInput?.state.text !== debounced ||
    suggestionState.lastInput.edition !== edition
  const totalQuery = useMemo<SearchQuery | null>(
    () =>
      typed ? { state: { ...DEFAULT_QUERY, unit: "card", text: typed }, edition, limit: 1 } : null,
    [typed, edition],
  )
  const totalState = useSearch(totalQuery)
  const suggestTotal =
    totalState.result.phase === "complete" &&
    totalState.lastInput?.state.text === typed &&
    totalState.lastInput.edition === edition
      ? totalState.result.page.total
      : 0
  const rows: SuggestRow[] = useMemo(
    () =>
      debounced !== ""
        ? (suggestionRows?.suggestions ?? []).map((item) => ({
            summary: item.summary,
            name: nameOf(item.summary),
            field: item.field,
          }))
        : recentResult.phase === "complete"
          ? (recentResult.recent ?? []).flatMap((item) =>
              item.summary ? [{ summary: item.summary, name: nameOf(item.summary) }] : [],
            )
          : [],
    [debounced, suggestionRows, recentResult, nameOf],
  )
  const suggestionImages = debounced !== "" ? suggestionState.images : recentResult.images
  const listOpen =
    focused &&
    readiness.phase === "ready" &&
    readiness.edition === edition &&
    readiness.searchReady !== false

  const pages = Math.max(1, entry.pages ?? 1)
  // The view is presentation only: switching it must not run the search again, so the query
  // handed to the search keeps its identity while everything but the view is unchanged.
  const { view: chosenView, ...conditions } = query
  const conditionsKey = formatQuery(conditions).toString()
  const searchState = useMemo(() => parseQuery(new URLSearchParams(conditionsKey)), [conditionsKey])
  const searchQuery = useMemo<SearchQuery>(
    () => ({ state: searchState, edition }),
    [searchState, edition],
  )
  const {
    view: listView,
    loading: pageLoading,
    images,
    imageRoot,
    error: queryError,
    progress,
    retry: retryView,
  } = useListView(searchQuery, pages)
  const page = listView?.page ?? null
  const positionQuery = JSON.stringify([location.pathname, conditionsKey, edition])
  const positionRef = useRef({ query: positionQuery, key: location.key })
  useLayoutEffect(() => {
    const previous = positionRef.current
    const changedQuery = previous.query !== positionQuery
    const returning = navigationType === NavigationType.Pop && previous.key !== location.key
    if (changedQuery || returning) {
      listScroll.current =
        navigationType === NavigationType.Pop
          ? (entry.listScroll ?? { top: 0, left: 0 })
          : { top: 0, left: 0 }
      restoreScroll.current = navigationType === NavigationType.Pop ? (entry.scrollY ?? 0) : 0
      const container = imageRoot.current?.querySelector<HTMLElement>("[data-list-scroll]")
      if (container) {
        container.scrollTop = listScroll.current.top
        container.scrollLeft = listScroll.current.left
      }
    }
    positionRef.current = { query: positionQuery, key: location.key }
  }, [positionQuery, location.key, navigationType, entry.listScroll, entry.scrollY, imageRoot])
  useLayoutEffect(() => {
    if (!page || pageLoading || restoreScroll.current === undefined) return
    window.scrollTo(0, restoreScroll.current)
    restoreScroll.current = undefined
  }, [page, pageLoading, positionQuery, location.key])
  const cellOf = useMemo(() => createCellReader(nameOf), [nameOf])
  const cells: GridCell[] = useMemo(
    () =>
      page?.items.flatMap((item) => {
        const cell = cellOf(item)
        return cell ? [cell] : []
      }) ?? [],
    [page, cellOf],
  )
  const background = code ? querySearch(searchState) : location.search
  const openContextRef = useRef({ pages, updateEntry, background })
  useLayoutEffect(() => {
    openContextRef.current = { pages, updateEntry, background }
  }, [pages, updateEntry, background])

  const openSuggestion = (index: number) => {
    const row = rows[index]
    if (!row || stale) return
    // Not stale here, so `typed` is the debounced text trimmed: the list URL never carries spaces.
    const fromSuggest = typed !== ""
    // The typed text becomes the list's URL first, so back returns to it with the string kept.
    const background = fromSuggest
      ? `?${urlParams({ ...DEFAULT_QUERY, text: typed }).toString()}`
      : location.search
    if (fromSuggest && background !== location.search) {
      void navigate(location.pathname + background, { replace: true, state: { pages: 1 } })
    }
    const state: CardEntryState = {
      background,
      source: fromSuggest ? "suggest" : "results",
      pages: 1,
      resultKey: row.summary.printingId,
      ...routeState(row.summary),
    }
    recentStore.push(row.summary.printingId)
    setEdit(null)
    setFocused(false)
    void navigate(cardPath(row.summary), { state })
  }
  const seeAll = () => {
    setFocused(false)
    inputRef.current?.blur()
    commit({ ...DEFAULT_QUERY, text: typed })
  }
  const activate = (index: number) => {
    if (index >= 0) openSuggestion(index)
    else if (typed !== "" && suggestTotal === 1) openSuggestion(0)
    else {
      setFocused(false)
      inputRef.current?.blur()
      commit({ ...query, text: typed })
    }
  }
  const openArmed = useEffectEvent(() => {
    if (suggestionResult.phase === "error" || totalState.result.phase === "error") {
      armed.current = null
      return
    }
    if (
      armed.current !== typed ||
      stale ||
      totalState.result.phase !== "complete" ||
      totalState.lastInput?.state.text !== typed ||
      totalState.lastInput.edition !== edition
    )
      return
    armed.current = null
    activate(-1)
  })
  useEffect(() => {
    openArmed()
  }, [typed, stale, totalState])

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.nativeEvent.isComposing) return
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      if (rows.length === 0) return
      event.preventDefault()
      setFocused(true)
      const step = event.key === "ArrowDown" ? 1 : -1
      setActive((active + step + rows.length) % rows.length)
      return
    }
    if (event.key === "Escape") {
      armed.current = null
      if (focused) {
        event.preventDefault()
        setFocused(false)
      }
      return
    }
    if (event.key !== "Enter") return
    event.preventDefault()
    if (active >= 0 && !stale) {
      openSuggestion(active)
      return
    }
    if (
      typed !== "" &&
      (stale ||
        totalState.result.phase !== "complete" ||
        totalState.lastInput?.state.text !== typed ||
        totalState.lastInput.edition !== edition)
    ) {
      armed.current = typed
      return
    }
    activate(active)
  }
  const openCell = useCallback((cell: GridCell): CardEntryState => {
    const current = openContextRef.current
    current.updateEntry({
      anchor: cell.key,
      pages: current.pages,
      listScroll: listScroll.current,
      scrollY: window.scrollY,
    })
    recentStore.push(cell.summary.printingId)
    return {
      background: current.background,
      ...routeState(cell.summary),
      source: "results",
      pages: current.pages,
      resultKey: cell.key,
    }
  }, [])

  const loadNextPage = useCallback(() => {
    updateEntry({ pages: pages + 1, listScroll: listScroll.current, scrollY: window.scrollY })
  }, [updateEntry, pages])

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
  const classLabel = (code: string) => quickBar.find((item) => item.code === code)?.label ?? code
  const filterCount = activeFilterCount(query)
  const hasConditions = query.text !== "" || filterCount > 0
  const view = chosenView ?? prefs.viewMode
  const unsupported = queryError?.kind === "unsupported-query"
  const pending = !queryError && page === null
  const artOf = useCallback(
    (cell: GridCell): ImageSource | undefined =>
      images?.artImage?.(cell.summary.printingId, cell.summary.faceId),
    [images],
  )

  const pagination =
    page && page.total > 0 ? (
      <PaginationTail
        unit={query.unit}
        automatic={prefs.infiniteScroll}
        requestKey={JSON.stringify([conditionsKey, edition, pages])}
        total={page.total}
        hasMore={page.items.length < page.total}
        loading={
          pageLoading || (!queryError && page.items.length < Math.min(pages * 60, page.total))
        }
        failed={Boolean(queryError)}
        onMore={loadNextPage}
        onRetry={() => {
          retryView()
          if (readiness.phase === "error") readiness.retry()
        }}
      />
    ) : null

  if (code && !set) {
    return (
      <div ref={imageRoot} className="flex flex-col gap-3 pt-2 pb-6">
        <h1 className="text-20 font-semibold">{code}</h1>
        <StatusStrip readiness={readiness} />
        {sets ? (
          <Notice tone="info" role="status">
            {t("sets.notFound")}
          </Notice>
        ) : (
          <SkeletonGrid />
        )}
      </div>
    )
  }

  return (
    <div ref={imageRoot} className="flex flex-col gap-3 pt-2 pb-6">
      {code ? (
        <h1 className="text-20 font-semibold">
          {set ? (set.name.translations[textLang] ?? set.name.original.text) : code}
        </h1>
      ) : (
        <h1 className="sr-only">{t("pages.cards")}</h1>
      )}
      <StatusStrip readiness={readiness} />
      <div className="relative z-20">
        <SearchBar
          value={input}
          onChange={(value) => {
            armed.current = null
            setInput(value)
            setFocused(true)
          }}
          onKeyDown={onKeyDown}
          onFocus={() => {
            setFocused(true)
          }}
          onBlur={() => {
            armed.current = null
            setFocused(false)
          }}
          inputRef={inputRef}
          listId={listId}
          expanded={listOpen}
          {...(active >= 0 ? { activeOptionId: optionId(listId, active) } : {})}
          filterCount={filterCount}
        />
        {focused && !listOpen && readiness.phase !== "error" && (
          <p role="status" className="mt-2 text-13 text-text-2">
            {t("home.searchPending")}
          </p>
        )}
        {listOpen && (
          <div className="absolute inset-x-0 top-full mt-1">
            <SuggestList
              id={listId}
              rows={rows}
              activeIndex={active}
              images={suggestionImages}
              onPick={openSuggestion}
              onHover={setActive}
              stale={stale}
              {...(debounced !== ""
                ? { query: { text: typed, total: suggestTotal, onSeeAll: seeAll } }
                : {
                    onClearRecent: () => {
                      recentStore.clear()
                    },
                  })}
            />
          </div>
        )}
      </div>
      {quickBar.length > 0 && (
        <ClassQuickBar
          label={t("search.classes")}
          classes={quickBar}
          selected={query.classes}
          onToggle={(code) => {
            commit(toggleClass(query, code))
          }}
        />
      )}
      {hasConditions && (
        <div
          role="group"
          aria-label={t("results.conditions")}
          className="flex flex-wrap items-center gap-2"
        >
          {query.text !== "" && (
            <Chip
              removeLabel={t("results.removeCondition", { label: query.text })}
              onRemove={() => {
                commit({ ...query, text: "" })
              }}
            >
              {query.text}
            </Chip>
          )}
          {query.classes.map((code) => (
            <Chip
              key={code}
              removeLabel={t("results.removeCondition", { label: classLabel(code) })}
              onRemove={() => {
                commit(toggleClass(query, code))
              }}
            >
              {classLabel(code)}
            </Chip>
          ))}
          <button
            type="button"
            onClick={() => {
              commit(DEFAULT_QUERY)
            }}
            className="h-8 rounded-pill px-2 text-13 font-semibold text-accent-text hover:bg-surface-2"
          >
            {t("results.clearConditions")}
          </button>
        </div>
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
          actions={
            <Button
              onClick={() => {
                retryView()
                // A failed background closure is fetched again only by a new load.
                readiness.retry()
              }}
            >
              {t("results.retry")}
            </Button>
          }
        >
          {t("results.loadFailed")}
        </Notice>
      )}
      {/* The status strip above says what is downloading; the list only shows its skeleton. */}
      {pending && readiness.phase !== "error" && (
        <>
          {readiness.phase === "ready" && (
            <p role="status" className="text-13 text-text-2">
              {progress ? t("results.searchProgress", progress) : t("results.dependenciesPending")}
            </p>
          )}
          <SkeletonGrid />
        </>
      )}
      {page !== null && page.total === 0 && (
        <div className="flex flex-col items-center gap-3 rounded-block border border-dashed border-border-strong px-4 py-10 text-center">
          <p className="text-16 font-semibold">{t("results.empty")}</p>
          <p className="text-13 text-text-3">{t("results.emptyHint")}</p>
          {hasConditions && (
            <Button
              onClick={() => {
                commit(DEFAULT_QUERY)
              }}
            >
              {t("results.clearAll")}
            </Button>
          )}
        </div>
      )}
      {page !== null && page.total > 0 && (
        <>
          <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
            <p className="text-13 text-text-2" aria-live="polite">
              <b className="font-semibold text-text-1 tabular-nums">
                {new Intl.NumberFormat(uiLanguage).format(page.total)}
              </b>{" "}
              {t(query.unit === "printing" ? "results.countPrintingUnit" : "results.countUnit")}
            </p>
            <div className="ml-auto flex items-center gap-2">
              <label className="flex items-center gap-1 text-13">
                <span>{t("results.sort")}</span>
                <select
                  aria-label={t("results.sort")}
                  value={query.sort}
                  className="h-8 rounded-control border border-border bg-surface-1 px-2 text-text-1"
                  onChange={(event) => {
                    const sort = QUERY_SORTS.find((sort) => sort === event.target.value)
                    if (sort) commit({ ...query, sort })
                  }}
                >
                  {QUERY_SORTS.map((sort) => (
                    <option key={sort} value={sort} disabled={sort === "date"}>
                      {t(`results.sortBy.${sort}`)}
                    </option>
                  ))}
                </select>
              </label>
              <button
                type="button"
                aria-label={t("results.descending")}
                aria-pressed={query.order === "desc"}
                className="h-8 rounded-control border border-border bg-surface-1 px-2 text-13"
                onClick={() => {
                  const { order, ...rest } = query
                  commit(order === "desc" ? rest : { ...rest, order: "desc" })
                }}
              >
                {query.order === "desc" ? "↓" : "↑"}
              </button>
              <Segmented
                size="sm"
                tone="accent"
                label={t("results.view")}
                options={VIEW_MODES.map((value) => ({ value, label: t(`options.view.${value}`) }))}
                value={view}
                onChange={(next) => {
                  updateEntry({ pages, listScroll: listScroll.current, scrollY: window.scrollY })
                  prefsStore.set({ viewMode: next })
                  setEdit(null)
                  setParams(urlParams({ ...query, view: next }), {
                    state: { ...entry, listScroll: listScroll.current, scrollY: window.scrollY },
                    preventScrollReset: true,
                  })
                }}
              />
            </div>
          </div>
          {view === "grid" && (
            <CardGrid
              cells={cells}
              images={images}
              onOpen={openCell}
              {...(entry.anchor === undefined ? {} : { anchor: entry.anchor })}
            />
          )}
          {view === "table" && (
            <CardTable
              cells={cells}
              images={images}
              onOpen={openCell}
              artOf={artOf}
              {...(entry.anchor === undefined ? {} : { anchor: entry.anchor })}
            />
          )}
          {view === "list" && (
            <CardList
              footer={pagination}
              sort={query.sort}
              descending={query.order === "desc"}
              onSort={(sort) => {
                const { order, ...rest } = query
                commit(
                  sort === query.sort && order !== "desc"
                    ? { ...rest, sort, order: "desc" }
                    : { ...rest, sort },
                )
              }}
              scrollPositionRef={listScroll}
              cells={cells}
              onOpen={openCell}
              {...(entry.anchor === undefined ? {} : { anchor: entry.anchor })}
            />
          )}
          {view !== "list" && pagination}
        </>
      )}
    </div>
  )
}
