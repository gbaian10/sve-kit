import {
  type KeyboardEvent,
  type ReactNode,
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react"
import { useTranslation } from "react-i18next"
import { useLocation } from "react-router"

import { withoutCardParam } from "../../app/card-overlay"
import { useDrawerSide, useSidebarLayout } from "../../app/cards-layout"
import { useListEntryState } from "../../app/listEntryState"
import { useListView, useSearchReadiness } from "../../app/snapshot"
import { useDebouncedValue } from "../../app/useDebouncedValue"
import type { GridCell } from "../../components/results/CardGrid"
import type { OpenOutcome } from "../../components/results/plain-click"
import { Segmented } from "../../components/ui/Segmented"
import { Switch } from "../../components/ui/Switch"
import type { CardSummary, SearchQuery } from "../../data"
import { printingPath } from "../../domain/cardRoute"
import { displayName, UI_TEXT_LANG } from "../../domain/nameDisplay"
import { DEFAULT_QUERY, NEUTRAL_CLASS, type QueryState } from "../../domain/query/model"
import { currentUiLanguage } from "../../i18n"
import { prefsStore, usePrefs } from "../../settings"
import { CardBrowser } from "../cards/CardBrowser"
import { useCardOverlay } from "../cards/CardOverlay"
import { useDeckEditor } from "./deck-editor"
import { PoolCellAction, PoolCellOverlay } from "./PoolCell"

export type PoolScope = "all" | "main" | "evolve" | "leader"

const SCOPE_LOCATIONS: Record<Exclude<PoolScope, "all">, readonly string[]> = {
  main: ["main"],
  evolve: ["evolve", "advance"],
  leader: ["leader"],
}
const DECK_LOCATIONS = ["main", "evolve", "advance", "leader"]
const DEBOUNCE_MS = 150
// The pool always lists one cell per card: copies count per card, never per printing.
const POOL_QUERY: QueryState = { ...DEFAULT_QUERY, unit: "card" }

/**
 * The cards page's browser with the deck builder's additions: scope tabs and the legal-only
 * switch at the toolbar's left end, a stepper in every view, the copies over each card. Its query
 * lives here, not in the URL, which names the deck; cards open in the centre.
 */
export function CardPool({
  scope,
  onScopeChange,
  onOpener,
  deckAction,
  sidebarDefaultCollapsed,
}: {
  readonly scope: PoolScope
  readonly onScopeChange: (scope: PoolScope) => void
  /** Receives this pool's card opener (null when it goes away), so the deck list can open cards. */
  readonly onOpener: (open: ((summary: CardSummary) => void) | null) => void
  /** The control in the open card's bottom bar. */
  readonly deckAction: (summary: CardSummary | undefined) => ReactNode
  readonly sidebarDefaultCollapsed: boolean
}) {
  const { t } = useTranslation()
  const editor = useDeckEditor()
  const { edition, basis } = editor.deck
  const prefs = usePrefs()
  const uiLanguage = currentUiLanguage(prefs.uiLanguage)
  const readiness = useSearchReadiness(edition)
  const location = useLocation()

  const [query, setQuery] = useState<QueryState>(POOL_QUERY)
  const [input, setInput] = useState("")
  const debounced = useDebouncedValue(input, DEBOUNCE_MS)
  const [legalOnly, setLegalOnly] = useState(true)
  const [pages, setPages] = useState(1)
  const [focused, setFocused] = useState(false)
  const inputRef = useRef<HTMLInputElement>(null)
  const listId = useId()
  const [sheetOpen, setSheetOpen] = useState(false)
  const sidebar = useSidebarLayout()
  // The filters take the edge opposite the card drawer, as on the cards page.
  const sidebarEdge = useDrawerSide() === "left" ? ("right" as const) : ("left" as const)
  const [sidebarChoice, setSidebarChoice] = useState<boolean | null>(null)
  const sidebarCollapsed = sidebarChoice ?? sidebarDefaultCollapsed

  const titleSets = useMemo(
    () => (basis.kind === "title" ? (editor.facts?.setsOfTitle(basis.titleCode) ?? []) : []),
    [basis, editor.facts],
  )
  // One cell per card whatever the panel sends: copies count per card, never per printing.
  const shown = useMemo<QueryState>(
    () => ({ ...query, unit: "card", text: debounced.trim() }),
    [query, debounced],
  )
  // By default the pool answers "what can go into this deck": the deck's class and neutral (or the
  // title's sets) and cards that go into a deck at all; the scope tab narrows it to one deck.
  const effective = useMemo<QueryState>(() => {
    const locations = scope === "all" ? shown.locations : [...SCOPE_LOCATIONS[scope]]
    if (!legalOnly) return { ...shown, locations }
    const legal = { ...shown, locations: locations.length === 0 ? DECK_LOCATIONS : locations }
    // A neutral deck takes neutral cards but a leader of any class (the deck check's rule).
    if (basis.kind === "class" && basis.classCode === NEUTRAL_CLASS && scope === "leader")
      return legal
    if (basis.kind === "class")
      return { ...legal, classes: [...new Set([basis.classCode, NEUTRAL_CLASS])].sort() }
    // Title decks keep to the sets that carry the title; shared sets such as PR also list cards of
    // other titles, which the deck's problem list then reports.
    return titleSets.length > 0 ? { ...legal, sets: [...titleSets] } : legal
  }, [shown, scope, legalOnly, basis, titleSets])
  const searchQuery = useMemo<SearchQuery>(
    () => ({ state: effective, edition }),
    [effective, edition],
  )
  const [pagesFor, setPagesFor] = useState(searchQuery)
  if (pagesFor !== searchQuery) {
    setPagesFor(searchQuery)
    setPages(1)
  }
  const {
    view: listView,
    loading: pageLoading,
    images,
    imageRoot,
    error: queryError,
    progress,
    retry,
  } = useListView(searchQuery, pages)
  const page = listView?.page ?? null

  const nameOf = useCallback(
    (summary: CardSummary) => displayName(summary.name, uiLanguage, prefs.nameDisplay),
    [uiLanguage, prefs.nameDisplay],
  )
  const cells: GridCell[] = useMemo(
    () =>
      page?.items.flatMap((item) =>
        item.summary
          ? [
              {
                key: item.key,
                summary: item.summary,
                name: nameOf(item.summary),
                to: printingPath(item.summary),
                state: null,
              },
            ]
          : [],
      ) ?? [],
    [page, nameOf],
  )

  const [entry] = useListEntryState()
  const listScroll = useRef({ top: 0, left: 0 })
  const overlay = useCardOverlay({
    entry,
    background: withoutCardParam(location.search),
    pages,
    unit: query.unit,
    cells,
    listScroll,
    root: imageRoot,
    onReturned: () => undefined,
    deckAction,
    // The issue's single-card view: a dialog over the builder, never a page that leaves it.
    presentation: "modal",
  })
  const openInPlace = overlay.openInPlace
  // The overlay's opener changes on every render; the deck list gets one stable opener instead.
  const latestOpen = useRef(openInPlace)
  useLayoutEffect(() => {
    latestOpen.current = openInPlace
  })
  useEffect(() => {
    onOpener((summary) => {
      latestOpen.current(summary, summary.printingId)
    })
    return () => {
      onOpener(null)
    }
  }, [onOpener])
  const onOpen = useCallback(
    (cell: GridCell): OpenOutcome => {
      openInPlace(cell.summary, cell.key)
      return { kind: "opened" }
    },
    [openInPlace],
  )

  const commit = (next: QueryState) => {
    // Choosing classes in the panel asks for more than this deck's classes: the switch turns off.
    if (legalOnly && basis.kind === "class" && next.classes.join() !== query.classes.join())
      setLegalOnly(false)
    if (next.text !== shown.text) setInput(next.text)
    setQuery(next)
  }
  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Escape") setInput("")
    if (event.key === "Escape" || event.key === "Enter") inputRef.current?.blur()
  }
  const view = prefs.viewMode
  const renderAction = useCallback(
    (cell: GridCell) => <PoolCellAction cell={cell} compact={view !== "grid"} />,
    [view],
  )
  const renderOverlay = useCallback((cell: GridCell) => <PoolCellOverlay cell={cell} />, [])

  const toolbarStart = (
    <>
      <Segmented
        size="sm"
        tone="accent"
        label={t("decks.pool.scope")}
        value={scope}
        options={[
          { value: "all", label: t("decks.pool.all") },
          { value: "main", label: t("decks.section.main") },
          { value: "evolve", label: t("decks.section.evolve") },
          { value: "leader", label: t("decks.leader") },
        ]}
        onChange={onScopeChange}
      />
      {/* A short label keeps the toolbar one row beside the deck column; the full text is the tooltip. */}
      <label
        className="flex items-center gap-2 text-12 whitespace-nowrap text-text-2"
        title={t("decks.pool.legalOnlyLong")}
      >
        <Switch
          size="sm"
          checked={legalOnly}
          onChange={setLegalOnly}
          label={t("decks.pool.legalOnlyLong")}
        />
        {t("decks.pool.legalOnly")}
      </label>
    </>
  )
  return (
    <div data-card-pool="">
      <CardBrowser
        imageRoot={imageRoot}
        readiness={readiness}
        edition={edition}
        uiLanguage={uiLanguage}
        textLang={UI_TEXT_LANG[uiLanguage]}
        query={shown}
        onQueryChange={commit}
        // The deck fixes the edition; the panel's switch has nothing to change.
        onEditionChange={() => undefined}
        onClearAll={() => {
          setInput("")
          setQuery(POOL_QUERY)
        }}
        search={{
          value: input,
          onChange: setInput,
          onKeyDown,
          onFocus: () => {
            setFocused(true)
          },
          onBlur: () => {
            setFocused(false)
          },
          inputRef,
          listId,
          expanded: false,
          below:
            focused && readiness.phase === "downloading" ? (
              <p role="status" className="mt-2 text-13 text-text-2">
                {t("home.searchPending")}
              </p>
            ) : null,
        }}
        results={{
          page,
          cells,
          images,
          error: queryError,
          progress,
          loading: pageLoading,
          pages,
          onMore: () => {
            setPages((count) => count + 1)
          },
          onRetry: () => {
            retry()
            if (readiness.phase === "error") readiness.retry()
          },
        }}
        // The count stays through scope and condition changes, so the toolbar never shifts.
        countScope={JSON.stringify([readiness.root, readiness.manifestHash, edition])}
        view={view}
        onViewChange={(next) => {
          prefsStore.set({ viewMode: next })
        }}
        onOpen={onOpen}
        selected={overlay.selected}
        listScroll={listScroll}
        sidebar={{
          layout: sidebar,
          edge: sidebarEdge,
          collapsed: sidebarCollapsed,
          onToggle: () => {
            setSidebarChoice(!sidebarCollapsed)
          },
        }}
        sheetOpen={sheetOpen}
        onSheetOpenChange={setSheetOpen}
        toolbarStart={toolbarStart}
        unitChoice={false}
        renderAction={renderAction}
        renderOverlay={renderOverlay}
      >
        {overlay.drawer}
        {overlay.modal}
      </CardBrowser>
    </div>
  )
}
