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
  useRecentSummaries,
  useRouteEdition,
  useSearch,
  useSearchReadiness,
  useWorkerSuggestions,
} from "../../app/snapshot"
import { useDebouncedValue } from "../../app/useDebouncedValue"
import { useMediaQuery } from "../../app/useMediaQuery"
import type { GridCell } from "../../components/results/CardGrid"
import type { OpenOutcome } from "../../components/results/plain-click"
import { optionId } from "../../components/search/suggest-ids"
import { SuggestList, type SuggestRow } from "../../components/search/SuggestList"
import { type CardSummary, type SearchPage, type SearchQuery } from "../../data"
import { printingPath } from "../../domain/cardRoute"
import { displayName, UI_TEXT_LANG } from "../../domain/nameDisplay"
import { formatQuery, parseQuery, querySearch } from "../../domain/query/codec"
import { DEFAULT_QUERY, type QueryState } from "../../domain/query/model"
import type { Region } from "../../domain/search"
import { currentUiLanguage } from "../../i18n"
import { prefsStore, recentStore, usePrefs, useRecent } from "../../settings"
import { CardBrowser } from "./CardBrowser"
import { useCardOverlay } from "./CardOverlay"

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

export function CardsPage() {
  const { t } = useTranslation()
  const prefs = usePrefs()
  const uiLanguage = currentUiLanguage(prefs.uiLanguage)
  const textLang = UI_TEXT_LANG[uiLanguage]
  const edition = useRouteEdition()
  const readiness = useSearchReadiness(edition)
  const [params, setParams] = useSearchParams()
  const query = useMemo(() => parseQuery(params), [params])
  const urlParams = (next: QueryState, nextEdition: Region = edition) => {
    const result = formatQuery(next)
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
  const background = withoutCardParam(location.search)
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

  const panelOpen = prefs.filterPanelOpen
  const wideEnoughForBoth = useMediaQuery(`(min-width: ${String(BOTH_PANES_MIN_PX)}px)`)
  const sidebar = useSidebarLayout()
  const drawerSide = useDrawerSide()
  const [sidebarChoice, setSidebarChoice] = useState<boolean | null>(null)
  // With a card open the sidebar folds unless the window is wide enough for both, or it was chosen.
  const sidebarEdge = drawerSide === "left" ? ("right" as const) : ("left" as const)
  const sidebarCollapsed = sidebarChoice ?? (cardOverlay.drawer !== null && !wideEnoughForBoth)
  const countScope = JSON.stringify([readiness.root, readiness.manifestHash, edition, query.unit])
  const view = chosenView ?? prefs.viewMode
  const searchBelow = (
    <>
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
    </>
  )
  return (
    <>
      <h1 className="sr-only">{t("pages.cards")}</h1>
      <CardBrowser
        imageRoot={imageRoot}
        readiness={readiness}
        edition={edition}
        uiLanguage={uiLanguage}
        textLang={textLang}
        query={query}
        onQueryChange={commit}
        onEditionChange={commitEdition}
        onClearAll={() => {
          commit(DEFAULT_QUERY)
        }}
        search={{
          value: input,
          onChange: (value) => {
            armed.current = null
            setInput(value)
            setFocused(true)
          },
          onKeyDown,
          onFocus: () => {
            setFocused(true)
          },
          onBlur: () => {
            armed.current = null
            setFocused(false)
          },
          inputRef,
          listId,
          expanded: listOpen,
          activeOptionId: active >= 0 ? optionId(listId, active) : undefined,
          below: searchBelow,
        }}
        results={{
          page,
          cells,
          images,
          error: queryError,
          progress,
          loading: pageLoading,
          pages,
          onMore: loadNextPage,
          onRetry: () => {
            retryView()
            // A failed background closure is fetched again only by a new load.
            readiness.retry()
          },
        }}
        countScope={countScope}
        view={view}
        onViewChange={(next) => {
          prefsStore.set({ viewMode: next })
          setEdit(null)
          commitList(
            urlParams({ ...query, view: next }),
            { ...listEntry, listScroll: listScroll.current, scrollY: window.scrollY },
            true,
          )
        }}
        onOpen={openCell}
        anchor={returnAnchor}
        selected={cardOverlay.selected}
        listScroll={listScroll}
        sidebar={{
          layout: sidebar,
          edge: sidebarEdge,
          collapsed: sidebarCollapsed,
          onToggle: () => {
            setSidebarChoice(!sidebarCollapsed)
          },
        }}
        sheetOpen={panelOpen}
        onSheetOpenChange={(open) => {
          prefsStore.set({ filterPanelOpen: open })
        }}
      >
        {cardOverlay.drawer}
        {cardOverlay.modal}
      </CardBrowser>
    </>
  )
}
