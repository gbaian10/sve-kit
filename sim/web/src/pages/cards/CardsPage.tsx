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
  Link as RouterLink,
  NavigationType,
  useLocation,
  useNavigate,
  useNavigationType,
  useParams,
  useSearchParams,
} from "react-router"

import { CARD_PARAM, withoutCardParam } from "../../app/card-overlay"
import {
  type CardEntryState,
  type ListEntryState,
  routeState,
  useListEntryState,
} from "../../app/listEntryState"
import {
  useListView,
  usePrintingOptions,
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
import { ListColumnPicker } from "../../components/results/ListColumnPicker"
import { PaginationTail } from "../../components/results/PaginationTail"
import type { OpenOutcome } from "../../components/results/plain-click"
import { SkeletonGrid } from "../../components/results/SkeletonGrid"
import { ClassQuickBar, type QuickBarClass } from "../../components/search/ClassQuickBar"
import { ConditionChips } from "../../components/search/ConditionChips"
import { vocabularyOptionLabel } from "../../components/search/facet-labels"
import { FilterPanel } from "../../components/search/FilterPanel"
import { KeywordFilter } from "../../components/search/KeywordFilter"
import { LocationFilter } from "../../components/search/LocationFilter"
import { RarityFilter } from "../../components/search/RarityFilter"
import { SearchBar } from "../../components/search/SearchBar"
import { SetFilter } from "../../components/search/SetFilter"
import { optionId } from "../../components/search/suggest-ids"
import { SuggestList, type SuggestRow } from "../../components/search/SuggestList"
import { TraitFilter } from "../../components/search/TraitFilter"
import { TypeFilter } from "../../components/search/TypeFilter"
import { Button } from "../../components/ui/Button"
import { Link } from "../../components/ui/Link"
import { Notice } from "../../components/ui/Notice"
import { Segmented } from "../../components/ui/Segmented"
import { type CardSummary, type ImageSource, type SearchPage, type SearchQuery } from "../../data"
import { printingPath } from "../../domain/cardRoute"
import { quickBarClasses } from "../../domain/classes"
import { leaderFrameTokens, tokenKindTokens } from "../../domain/deckLocation"
import { displayName, UI_TEXT_LANG } from "../../domain/nameDisplay"
import { editionUrl, formatQuery, parseQuery, querySearch } from "../../domain/query/codec"
import {
  activeFilterCount,
  DEFAULT_QUERY,
  NEUTRAL_CLASS,
  QUERY_SORTS,
  type QueryState,
  toggleClass,
  VIEW_MODES,
} from "../../domain/query/model"
import { prStyleTokens } from "../../domain/rarityFilter"
import type { Region } from "../../domain/search"
import { setCaption } from "../../domain/sets"
import { currentUiLanguage } from "../../i18n"
import { prefsStore, recentStore, usePrefs, useRecent } from "../../settings"
import { SetHeader } from "../sets/SetHeader"
import { useCardOverlay } from "./CardOverlay"

const DEBOUNCE_MS = 150
/** How long a cell returned to from a card stays outlined (#522: about a second). */
const RETURN_FLASH_MS = 1000
/** Frames to wait for the returned-to cell to render before giving up on focusing it. */
const RETURN_FRAMES = 30

function resultCell(root: HTMLElement | null, key: string): HTMLElement | undefined {
  return [...(root?.querySelectorAll<HTMLElement>("a[data-result-key]") ?? [])].find(
    (link) => link.getAttribute("data-result-key") === key,
  )
}

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
  const { sets, quickSets, setCounts, rarities } = useSetOptions()
  const printingOptions = usePrintingOptions()
  const set = code ? sets?.find((set) => set.publicCode === code) : undefined
  const query = useMemo(() => {
    const parsed = parseQuery(params)
    return code && !params.has("set") ? { ...parsed, sets: set ? [set.code] : [] } : parsed
  }, [params, set, code])
  const urlParams = (next: QueryState, nextEdition: Region = edition) => {
    const result = formatQuery(next)
    // A set page's own set is implied by its path, so each state keeps one URL.
    if (set && next.sets.length === 1 && next.sets[0] === set.code) result.delete("set")
    if (nextEdition === "en") result.set("ed", "en")
    return result
  }
  const [entry, updateEntry] = useListEntryState()
  const { overlay, ...listEntry } = entry
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
  const panelId = useId()
  const debounced = useDebouncedValue(input, DEBOUNCE_MS)
  const active =
    focused && highlight !== null && highlight.text === debounced ? highlight.index : -1
  const setActive = (index: number) => {
    setHighlight({ text: debounced, index })
  }

  // New conditions or a new view push a list entry. With a card open beside the results the card
  // stays, but history keeps one closable card entry: the new list replaces the card's entry and
  // the card is pushed again on top, so back closes the card and back again is the old list.
  const openCardParam = params.get(CARD_PARAM)
  const commitList = (next: URLSearchParams, state: ListEntryState, preventScrollReset = false) => {
    const searchOf = (value: URLSearchParams) => {
      const text = value.toString()
      return text === "" ? "" : `?${text}`
    }
    if (openCardParam === null) {
      updateEntry({ pages, listScroll: listScroll.current, scrollY: window.scrollY })
      setParams(next, { state, preventScrollReset })
      return
    }
    void navigate(
      { pathname: location.pathname, search: searchOf(next) },
      { replace: true, state, preventScrollReset },
    )
    const withCard = new URLSearchParams(next)
    withCard.set(CARD_PARAM, openCardParam)
    void navigate(
      { pathname: location.pathname, search: searchOf(withCard) },
      { state: { ...state, overlay: { ...overlay, pushed: true } }, preventScrollReset: true },
    )
  }
  // Changing a condition leaves the window where the user is (usually inside the filter panel):
  // neither the router's scroll reset nor the list's own restore may jump it to the top.
  const commit = (next: QueryState) => {
    setEdit(null)
    commitList(urlParams(next), {}, true)
  }
  // The list's edition lives in `ed`; the preference follows so other pages open the same edition.
  const commitEdition = (next: Region) => {
    prefsStore.set({ cardEdition: next })
    setEdit(null)
    commitList(urlParams(query, next), {}, true)
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

  // The view is presentation only: switching it must not run the search again, so the query
  // handed to the search keeps its identity while everything but the view is unchanged.
  const { view: chosenView, ...conditions } = query
  const conditionsKey = formatQuery(conditions).toString()
  const positionQuery = JSON.stringify([location.pathname, conditionsKey, edition])
  const openCard = params.get(CARD_PARAM)
  // Going back from a card open beside the results lands on the list's own entry, which still has
  // the pages it had when the card opened; pages loaded beside the card are kept and written back.
  const entryPages = Math.max(1, entry.pages ?? 1)
  const [shownPages, setShownPages] = useState({
    key: location.key,
    card: openCard,
    query: positionQuery,
    pages: entryPages,
  })
  let pages = shownPages.pages
  if (shownPages.key !== location.key) {
    const keep =
      navigationType === NavigationType.Pop &&
      shownPages.card !== null &&
      openCard === null &&
      shownPages.query === positionQuery &&
      shownPages.pages > entryPages
    pages = keep ? shownPages.pages : entryPages
    setShownPages({ key: location.key, card: openCard, query: positionQuery, pages })
  }
  useEffect(() => {
    if (pages > entryPages) updateEntry({ pages })
  }, [pages, entryPages, updateEntry])
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
  const positionRef = useRef({ query: positionQuery, key: location.key, card: openCard })
  useLayoutEffect(() => {
    const previous = positionRef.current
    const changedQuery = previous.query !== positionQuery
    // Back that only closes or changes the open card leaves the list where the user scrolled it.
    const returning =
      navigationType === NavigationType.Pop &&
      previous.key !== location.key &&
      (changedQuery || previous.card === openCard)
    // A changed condition only rewinds the list's own scrollport; only a return restores the
    // window position.
    if (changedQuery || returning) {
      listScroll.current =
        navigationType === NavigationType.Pop
          ? (entry.listScroll ?? { top: 0, left: 0 })
          : { top: 0, left: 0 }
      if (navigationType === NavigationType.Pop) restoreScroll.current = entry.scrollY ?? 0
      const container = imageRoot.current?.querySelector<HTMLElement>("[data-list-scroll]")
      if (container) {
        container.scrollTop = listScroll.current.top
        container.scrollLeft = listScroll.current.left
      }
    }
    positionRef.current = { query: positionQuery, key: location.key, card: openCard }
  }, [
    positionQuery,
    openCard,
    location.key,
    navigationType,
    entry.listScroll,
    entry.scrollY,
    imageRoot,
  ])
  // The cell a card returned to is flashed for a moment, whether the card was a page or beside
  // the results.
  const [flash, setFlash] = useState<string | undefined>(undefined)
  const flashTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const flashCell = useCallback((key: string) => {
    clearTimeout(flashTimer.current)
    setFlash(key)
    flashTimer.current = setTimeout(() => {
      setFlash(undefined)
    }, RETURN_FLASH_MS)
  }, [])
  useEffect(
    () => () => {
      clearTimeout(flashTimer.current)
    },
    [],
  )
  // Back from a card page mounts the list again: once its pages and scroll are back, focus returns
  // to the cell that opened the card. Rows render around the scroll position, so wait for the cell.
  const returnTo = useRef(navigationType === NavigationType.Pop ? entry.anchor : undefined)
  useLayoutEffect(() => {
    if (!page || pageLoading || restoreScroll.current === undefined) return
    window.scrollTo(0, restoreScroll.current)
    restoreScroll.current = undefined
    const anchor = returnTo.current
    returnTo.current = undefined
    if (anchor === undefined) return
    let tries = 0
    const settle = () => {
      const cell = resultCell(imageRoot.current, anchor)
      if (cell) {
        cell.focus({ preventScroll: true })
        flashCell(anchor)
      } else if (++tries < RETURN_FRAMES) requestAnimationFrame(settle)
    }
    requestAnimationFrame(settle)
  }, [page, pageLoading, positionQuery, location.key, imageRoot, flashCell])
  const cellOf = useMemo(() => createCellReader(nameOf), [nameOf])
  const cells: GridCell[] = useMemo(
    () =>
      page?.items.flatMap((item) => {
        const cell = cellOf(item)
        return cell ? [cell] : []
      }) ?? [],
    [page, cellOf],
  )
  const background = code ? querySearch(searchState) : withoutCardParam(location.search)
  const cardOverlay = useCardOverlay({
    entry,
    background: querySearch(searchState),
    pages,
    unit: query.unit,
    cells,
    listScroll,
    root: imageRoot,
    onReturned: flashCell,
  })
  const openInPlace = cardOverlay.openInPlace
  const returnAnchor = cardOverlay.open ? undefined : flash
  const openContextRef = useRef({ pages, updateEntry, background, openInPlace })
  useLayoutEffect(() => {
    openContextRef.current = { pages, updateEntry, background, openInPlace }
  }, [pages, updateEntry, background, openInPlace])

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
  const openCell = useCallback((cell: GridCell): OpenOutcome => {
    const current = openContextRef.current
    recentStore.push(cell.summary.printingId)
    if (current.openInPlace(cell.summary, cell.key)) return { kind: "opened" }
    current.updateEntry({
      anchor: cell.key,
      pages: current.pages,
      listScroll: listScroll.current,
      scrollY: window.scrollY,
    })
    const state: CardEntryState = {
      background: current.background,
      ...routeState(cell.summary),
      source: "results",
      pages: current.pages,
      resultKey: cell.key,
    }
    return { kind: "navigate", state }
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
  const classLabel = (code: string | null) =>
    code === null
      ? t("search.neutral")
      : (quickBar.find((item) => item.code === code)?.label ?? code)
  const typeLabel = (code: string) => {
    const option = printingOptions?.type.types.find((item) => item.code === code)
    return option ? vocabularyOptionLabel(option.name, textLang) : code
  }
  // A set page's own set is the page, not a condition: no chip, no badge count. The chips still
  // edit the full query, so removing another chip never drops the implied set from a multi-select.
  const countedConditions = set
    ? { ...query, sets: query.sets.filter((item) => item !== set.code) }
    : query
  const filterCount = activeFilterCount(countedConditions)
  const hasConditions = countedConditions.text !== "" || filterCount > 0
  const panelOpen = prefs.filterPanelOpen
  const countScope = JSON.stringify([readiness.root, readiness.manifestHash, edition, query.unit])
  const [lastCount, setLastCount] = useState({ scope: countScope, total: page?.total ?? null })
  // Keep the count through a new query, but never carry it into another dataset or result unit.
  if (lastCount.scope !== countScope || (page !== null && lastCount.total !== page.total)) {
    setLastCount({ scope: countScope, total: page?.total ?? null })
  }
  const total =
    page?.total ?? (lastCount.scope === countScope && !queryError ? lastCount.total : null)
  const resultCount =
    total !== null ? (
      <div
        className="shrink-0 text-right text-13 whitespace-nowrap text-text-2"
        aria-live="polite"
        aria-busy={page === null}
      >
        <p>
          <b className="inline-block min-w-14 font-semibold text-text-1 tabular-nums">
            {new Intl.NumberFormat(uiLanguage).format(total)}
          </b>{" "}
          {query.unit === "printing"
            ? t("results.countPrintingUnit")
            : query.unit === "art"
              ? t("results.countArtUnit")
              : t("results.countUnit")}
        </p>
        {Object.keys(query.mechanics).length > 0 &&
          printingOptions?.keyword.coverage === "partial" && (
            <p className="text-11">{t("filters.keywordIncomplete")}</p>
          )}
      </div>
    ) : null
  const view = chosenView ?? prefs.viewMode
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
    // The set list only holds this edition's families, so the code may well exist in the other
    // edition (an EN-only product opened under JP): offer that switch instead of a dead end.
    const otherEdition: Region = edition === "jp" ? "en" : "jp"
    return (
      <div ref={imageRoot} className="flex flex-col gap-3 pt-2 pb-6">
        <h1 className="text-20 font-semibold">{code}</h1>
        <StatusStrip readiness={readiness} />
        {sets ? (
          <Notice
            tone="info"
            role="status"
            actions={
              <>
                <RouterLink
                  to={editionUrl(location.pathname, otherEdition)}
                  className="text-13 text-accent-text underline-offset-2 hover:underline"
                  onClick={() => {
                    prefsStore.set({ cardEdition: otherEdition })
                  }}
                >
                  {t("sets.switchEdition", { edition: t(`options.edition.${otherEdition}`) })}
                </RouterLink>
                <Link to={editionUrl("/sets", edition)} className="text-13">
                  {t("home.sets.all")}
                </Link>
              </>
            }
          >
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
      {set ? (
        <SetHeader set={set} edition={edition} textLang={textLang} locale={uiLanguage} />
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
          filtersOpen={panelOpen}
          filtersId={panelId}
          onToggleFilters={() => {
            prefsStore.set({ filterPanelOpen: !panelOpen })
          }}
          summary={resultCount}
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
      {/* The class row is the one filter outside the panel: always visible, so the page is never bare. */}
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
      {/* The chip row sits with the class row, above the panel, so it stays in view either way. */}
      <ConditionChips
        query={query}
        impliedSet={set?.code}
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
        onChange={commit}
        onClear={() => {
          commit(DEFAULT_QUERY)
        }}
      />
      <FilterPanel
        id={panelId}
        open={panelOpen}
        query={query}
        onChange={commit}
        edition={edition}
        onEditionChange={commitEdition}
        resultTotal={page?.total}
        onClose={() => {
          prefsStore.set({ filterPanelOpen: false })
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
              onChange={(rarities) => {
                commit({ ...query, rarities })
              }}
            />
          ),
          sets: (
            <>
              {/* A set page's set comes from its path, so the dropdown would only show one choice. */}
              {!code && (
                <SetFilter
                  sets={sets}
                  quickSets={quickSets}
                  counts={setCounts}
                  textLang={textLang}
                  selected={query.sets}
                  onChange={(sets) => {
                    commit({ ...query, sets })
                  }}
                />
              )}
            </>
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
          keywords: (
            <KeywordFilter
              options={printingOptions?.keyword}
              textLang={textLang}
              edition={edition}
              selected={query.mechanics}
              onChange={(mechanics) => {
                commit({ ...query, mechanics })
              }}
            />
          ),
        }}
      />
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
              {view === "list" && <ListColumnPicker />}
              <div className="ml-auto flex flex-wrap items-center gap-2">
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
                      <option key={sort} value={sort}>
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
                  options={VIEW_MODES.map((value) => ({
                    value,
                    label: t(`options.view.${value}`),
                  }))}
                  value={view}
                  onChange={(next) => {
                    prefsStore.set({ viewMode: next })
                    setEdit(null)
                    commitList(
                      urlParams({ ...query, view: next }),
                      { ...listEntry, listScroll: listScroll.current, scrollY: window.scrollY },
                      true,
                    )
                  }}
                />
              </div>
            </div>
            {view === "grid" && (
              <CardGrid
                cells={cells}
                images={images}
                onOpen={openCell}
                {...(returnAnchor === undefined ? {} : { anchor: returnAnchor })}
                {...(cardOverlay.selected === undefined ? {} : { selected: cardOverlay.selected })}
              />
            )}
            {view === "table" && (
              <CardTable
                typeLabel={typeLabel}
                cells={cells}
                images={images}
                onOpen={openCell}
                artOf={artOf}
                {...(returnAnchor === undefined ? {} : { anchor: returnAnchor })}
                {...(cardOverlay.selected === undefined ? {} : { selected: cardOverlay.selected })}
              />
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
                  commit(
                    sort === query.sort && order !== "desc"
                      ? { ...rest, sort, order: "desc" }
                      : { ...rest, sort },
                  )
                }}
                scrollPositionRef={listScroll}
                cells={cells}
                onOpen={openCell}
                {...(returnAnchor === undefined ? {} : { anchor: returnAnchor })}
                {...(cardOverlay.selected === undefined ? {} : { selected: cardOverlay.selected })}
              />
            )}
            {view !== "list" && pagination}
          </>
        )}
      </div>
      {cardOverlay.drawer}
      {cardOverlay.modal}
    </div>
  )
}
