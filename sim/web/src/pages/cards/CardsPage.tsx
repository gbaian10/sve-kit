import { ListFilter, SlidersHorizontal } from "lucide-react"
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
import { BOTH_PANES_MIN_PX, useDrawerSide, useSidebarLayout } from "../../app/cards-layout"
import {
  type CardEntryState,
  forgetPagesBeside,
  keepPagesBeside,
  type ListEntryState,
  newListId,
  pagesBeside,
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
import { useMediaQuery } from "../../app/useMediaQuery"
import { CardGrid, type GridCell } from "../../components/results/CardGrid"
import { CardList, CardTable } from "../../components/results/CardRows"
import { ListColumnPicker } from "../../components/results/ListColumnPicker"
import { PaginationTail } from "../../components/results/PaginationTail"
import type { OpenOutcome } from "../../components/results/plain-click"
import { SkeletonGrid } from "../../components/results/SkeletonGrid"
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
import { RarityFilter } from "../../components/search/RarityFilter"
import { SearchBar } from "../../components/search/SearchBar"
import { SetFilter } from "../../components/search/SetFilter"
import { optionId } from "../../components/search/suggest-ids"
import { SuggestList, type SuggestRow } from "../../components/search/SuggestList"
import { TraitFilter } from "../../components/search/TraitFilter"
import { TypeFilter } from "../../components/search/TypeFilter"
import { Button } from "../../components/ui/Button"
import { cn } from "../../components/ui/cn"
import { Link } from "../../components/ui/Link"
import { Notice } from "../../components/ui/Notice"
import { Segmented } from "../../components/ui/Segmented"
import { type CardSummary, type ImageSource, type SearchPage, type SearchQuery } from "../../data"
import { CREDITED_ARTIST, UNCREDITED_ARTIST } from "../../domain/artistFilter"
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
import { CompactHeaderMenu } from "./CompactHeaderMenu"
import { FilterSheet } from "./FilterSheet"
import { FilterSidebar } from "./FilterSidebar"
import { sidebarGridClass } from "./sidebar-layout"
import { useCompactHeader } from "./useCompactHeader"

const DEBOUNCE_MS = 150
/** How long a cell returned to from a card stays outlined (#522: about a second). */
const RETURN_FLASH_MS = 1000
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

// Clip only the gutter backdrop, so suggestions can extend below the sticky bar.
const FULL_BLEED =
  "isolate before:pointer-events-none before:absolute before:inset-0 before:-z-10 before:bg-bg before:shadow-[0_0_0_100vmax_var(--color-bg)] before:[clip-path:inset(0_-100vmax)]"

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
      setParams(next, { state: { ...state, listId: newListId() }, preventScrollReset })
      return
    }
    const listed: ListEntryState = { ...state, listId: newListId() }
    void navigate(
      { pathname: location.pathname, search: searchOf(next) },
      { replace: true, state: listed, preventScrollReset },
    )
    const withCard = new URLSearchParams(next)
    withCard.set(CARD_PARAM, openCardParam)
    void navigate(
      { pathname: location.pathname, search: searchOf(withCard) },
      {
        state: { ...listed, overlay: { ...overlay, listId: listed.listId, pushed: true } },
        preventScrollReset: true,
      },
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
  // Pages loaded while a card was open beside the results were written to the card's entry; the
  // list's own entry takes them up once it is current again, however the card was left.
  const entryPages = Math.max(1, entry.pages ?? 1)
  const carried = openCard === null ? pagesBeside(entry.listId) : undefined
  const pages = Math.max(entryPages, carried ?? 0)
  const entryListId = entry.listId
  useEffect(() => {
    if (carried === undefined || entryListId === undefined) return
    if (carried > entryPages) updateEntry({ pages: carried })
    forgetPagesBeside(entryListId)
  }, [carried, entryPages, entryListId, updateEntry])
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
  // A pages write replaces the router key, so a return follows the list's stable id.
  // Rows render around the restored scroll position; wait until the target cell exists.
  const returnEntryId = entry.listId ?? location.key
  const returnTo = useRef({
    id: returnEntryId,
    query: positionQuery,
    anchor: navigationType === NavigationType.Pop ? entry.anchor : undefined,
  })
  useLayoutEffect(() => {
    if (
      returnTo.current.id !== returnEntryId ||
      returnTo.current.query !== positionQuery ||
      openCard !== null
    )
      returnTo.current.anchor = undefined
    if (!page || pageLoading) return
    if (restoreScroll.current !== undefined) {
      window.scrollTo(0, restoreScroll.current)
      restoreScroll.current = undefined
    }
    const anchor = returnTo.current.anchor
    if (anchor === undefined) return
    let tries = 0
    let frame: number
    const settle = () => {
      const cell = resultCell(imageRoot.current, anchor)
      if (cell) {
        returnTo.current.anchor = undefined
        cell.focus({ preventScroll: true })
        flashCell(anchor)
      } else if (++tries < RETURN_FRAMES) frame = requestAnimationFrame(settle)
      else returnTo.current.anchor = undefined
    }
    frame = requestAnimationFrame(settle)
    return () => {
      cancelAnimationFrame(frame)
    }
  }, [
    page,
    pageLoading,
    positionQuery,
    location.key,
    returnEntryId,
    openCard,
    imageRoot,
    flashCell,
  ])
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
    if (fromSuggest) {
      const position = { anchor: row.summary.printingId, scrollY: window.scrollY }
      // Beside or over the results the card opens on the typed text's list; as a page it follows.
      if (
        openInPlace(row.summary, row.summary.printingId, {
          search: background,
          list: { pages: 1, ...position, listScroll: listScroll.current },
        })
      ) {
        recentStore.push(row.summary.printingId)
        setEdit(null)
        setFocused(false)
        return
      }
      if (background !== location.search) {
        void navigate(location.pathname + background, {
          replace: true,
          state: { pages: 1, ...position },
        })
      } else updateEntry({ ...position, listScroll: listScroll.current })
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

  const overlayListId = overlay?.pushed ? overlay.listId : undefined
  const loadNextPage = useCallback(() => {
    if (overlayListId !== undefined) keepPagesBeside(overlayListId, pages + 1)
    updateEntry({ pages: pages + 1, listScroll: listScroll.current, scrollY: window.scrollY })
  }, [updateEntry, pages, overlayListId])

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
  const wideEnoughForBoth = useMediaQuery(`(min-width: ${String(BOTH_PANES_MIN_PX)}px)`)
  const sidebar = useSidebarLayout()
  const drawerSide = useDrawerSide()
  const { sentinelRef, headRef, compact, narrow, foldedGap } = useCompactHeader(sidebar)
  const headerMenus = compact && narrow
  const [sidebarChoice, setSidebarChoice] = useState<boolean | null>(null)
  // With a card open the sidebar folds unless the window is wide enough for both, or it was chosen.
  const sidebarEdge = drawerSide === "left" ? ("right" as const) : ("left" as const)
  const sidebarCollapsed = sidebarChoice ?? (cardOverlay.drawer !== null && !wideEnoughForBoth)
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
        className={cn(
          "shrink-0 text-right text-13 whitespace-nowrap text-text-2",
          compact && "@max-[640px]:hidden",
        )}
        aria-live="polite"
        aria-busy={page === null}
      >
        <p>
          <b className="inline-block min-w-14 font-semibold text-text-1 tabular-nums">
            {new Intl.NumberFormat(uiLanguage).format(total)}
          </b>{" "}
          <span className="max-[479px]:hidden">
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
                ? t("filters.keywordUnknownCount", { count: printingOptions.keyword.unknownCards })
                : t("filters.keywordIncomplete")}
            </p>
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
  const setsBar = !code && (
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
      sidebar={sidebar}
      id={panelId}
      open={panelOpen}
      query={query}
      onChange={commit}
      edition={edition}
      onEditionChange={commitEdition}
      restrictions={printingOptions?.restrictions}
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
  )
  const conditionChips = (
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
      onChange={commit}
      onClear={() => {
        commit(DEFAULT_QUERY)
      }}
    />
  )
  const sortRow =
    page !== null && page.total > 0 ? (
      <div
        className={cn(
          "flex gap-x-3 gap-y-2",
          headerMenus ? "flex-col items-stretch" : "items-center",
          !compact && "flex-wrap",
        )}
      >
        {view === "list" && <ListColumnPicker inline={headerMenus} />}
        <div
          className={cn(
            "flex items-center gap-2",
            !headerMenus && "ml-auto",
            (!compact || headerMenus) && "flex-wrap",
          )}
        >
          <label className="flex items-center gap-1 text-13">
            <span className={cn(compact && "@max-[640px]:hidden")}>{t("results.sort")}</span>
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
    ) : null
  return (
    <div ref={imageRoot} className="flex flex-col gap-3 pt-2 pb-6">
      {set ? (
        <SetHeader set={set} edition={edition} textLang={textLang} locale={uiLanguage} />
      ) : (
        <h1 className="sr-only">{t("pages.cards")}</h1>
      )}
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
          sidebar
            ? cn("grid items-start gap-6", sidebarGridClass(sidebarEdge, sidebarCollapsed))
            : "flex flex-col gap-3"
        }
      >
        {sidebar && (
          <FilterSidebar
            edge={sidebarEdge}
            collapsed={sidebarCollapsed}
            onToggle={() => {
              setSidebarChoice(!sidebarCollapsed)
            }}
            filterCount={filterCount}
          >
            {classBar}
            {setsBar}
            {panelEl}
          </FilterSidebar>
        )}
        <div className="flex min-w-0 flex-col gap-3">
          {sidebar && <div ref={sentinelRef} aria-hidden="true" className="-mb-3 h-px" />}
          <div
            ref={headRef}
            style={foldedGap > 0 ? { marginBottom: foldedGap } : undefined}
            className={cn(
              sidebar ? `@container sticky top-13 z-30 flex gap-3 ${FULL_BLEED}` : "contents",
              compact ? "h-14 flex-row items-center border-b border-border" : "flex-col",
            )}
          >
            <div
              className={cn(
                "relative z-20",
                !sidebar && `sticky top-13 z-30 py-2 ${FULL_BLEED}`,
                compact && "max-w-96 flex-1 basis-40",
                compact && (headerMenus ? "min-w-0" : "min-w-52"),
              )}
            >
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
                {...(sidebar ? {} : { filterCount })}
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
            {headerMenus ? (
              <>
                {hasConditions && (
                  <CompactHeaderMenu
                    label={t("results.conditions")}
                    icon={<ListFilter aria-hidden="true" className="size-4" />}
                    fallbackFocusRef={inputRef}
                  >
                    {conditionChips}
                  </CompactHeaderMenu>
                )}
                {sortRow && (
                  <CompactHeaderMenu
                    label={`${t("results.sort")} / ${t("results.view")}`}
                    icon={<SlidersHorizontal aria-hidden="true" className="size-4" />}
                    fallbackFocusRef={inputRef}
                  >
                    {sortRow}
                  </CompactHeaderMenu>
                )}
              </>
            ) : (
              <>
                <div
                  className={cn(
                    compact &&
                      "min-w-0 flex-1 overflow-x-auto [&>[role=group]]:flex-nowrap [&>[role=group]>*]:shrink-0",
                  )}
                >
                  {conditionChips}
                </div>
                {sidebar && sortRow}
              </>
            )}
          </div>
          {!sidebar && (
            <FilterSheet
              open={panelOpen}
              onClose={() => {
                prefsStore.set({ filterPanelOpen: false })
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
                {!sidebar && sortRow}
                {view === "grid" && (
                  <CardGrid
                    cells={cells}
                    images={images}
                    onOpen={openCell}
                    {...(returnAnchor === undefined ? {} : { anchor: returnAnchor })}
                    {...(cardOverlay.selected === undefined
                      ? {}
                      : { selected: cardOverlay.selected })}
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
                    {...(cardOverlay.selected === undefined
                      ? {}
                      : { selected: cardOverlay.selected })}
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
                    {...(cardOverlay.selected === undefined
                      ? {}
                      : { selected: cardOverlay.selected })}
                  />
                )}
                {view !== "list" && pagination}
              </>
            )}
          </div>
        </div>
      </div>
      {cardOverlay.drawer}
      {cardOverlay.modal}
    </div>
  )
}
