import { type KeyboardEvent, useCallback, useEffect, useId, useMemo, useRef, useState } from "react"
import { useTranslation } from "react-i18next"
import { useLocation, useNavigate, useSearchParams } from "react-router"

import { useEffectPreviews, useTextContext } from "../../app/effectPreviews"
import { type CardEntryState, useListEntryState } from "../../app/listEntryState"
import { useCatalog, useImageIndex } from "../../app/snapshot"
import { useDebouncedValue } from "../../app/useDebouncedValue"
import { useOnline } from "../../app/useOnline"
import { chipsFor } from "../../components/filters/chips"
import { FilterChips } from "../../components/filters/FilterChips"
import { FilterSheet } from "../../components/filters/FilterSheet"
import { CardGrid, type GridCell } from "../../components/results/CardGrid"
import { ControlBar } from "../../components/results/ControlBar"
import { ListView } from "../../components/results/ListView"
import { SkeletonGrid } from "../../components/results/SkeletonGrid"
import { TableView } from "../../components/results/TableView"
import { ClassQuickBar, type QuickBarClass } from "../../components/search/ClassQuickBar"
import { SearchBar } from "../../components/search/SearchBar"
import { optionId } from "../../components/search/suggest-ids"
import { SuggestList, type SuggestRow } from "../../components/search/SuggestList"
import { Button } from "../../components/ui/Button"
import type { CardSummary } from "../../data"
import { quickBarClasses } from "../../domain/classes"
import { displayName, UI_TEXT_LANG } from "../../domain/nameDisplay"
import { formatQuery, parseQuery } from "../../domain/query/codec"
import {
  activeFilterCount,
  DEFAULT_QUERY,
  NEUTRAL_CLASS,
  type QueryState,
  toggleClass,
  type ViewMode,
} from "../../domain/query/model"
import type { Suggestion } from "../../domain/search"
import { currentUiLanguage } from "../../i18n"
import { prefsStore, recentStore, usePrefs, useRecent } from "../../settings"

const PAGE_SIZE = 60
const SUGGEST_LIMIT = 8
const DEBOUNCE_MS = 150

function cardPath(summary: CardSummary): string {
  return `/cards/${encodeURIComponent(summary.cardNo)}`
}

export interface CardsPageProps {
  /** Render the list for this search string instead of the URL (the card overlay's background). */
  readonly search?: string
  readonly pages?: number
  /** Background under an overlay: no interaction, no focus. */
  readonly inert?: boolean
}

// The search page: the URL holds the query, the input holds what is being typed, and the suggest
// list follows the typed text (debounced) until Enter or "see all" commits it to the URL.
export function CardsPage({ search, pages: fixedPages, inert = false }: CardsPageProps = {}) {
  const { t } = useTranslation()
  const prefs = usePrefs()
  const uiLanguage = currentUiLanguage(prefs.uiLanguage)
  const textLang = UI_TEXT_LANG[uiLanguage]
  const edition = prefs.cardEdition
  const { client, status, catalog } = useCatalog()
  const images = useImageIndex(client, catalog !== null)
  const [urlParams, setParams] = useSearchParams()
  const params = useMemo(
    () => (search === undefined ? urlParams : new URLSearchParams(search)),
    [search, urlParams],
  )
  const query = useMemo(() => parseQuery(params), [params])
  const [entry, updateEntry] = useListEntryState()
  const navigate = useNavigate()
  const location = useLocation()
  const recent = useRecent()

  // The typed text is a draft on top of one history entry: any navigation (commit, back, forward)
  // changes `location.key`, the draft stops applying and the input shows the URL's text again.
  const [edit, setEdit] = useState<{ readonly key: string; readonly value: string } | null>(null)
  const input = edit !== null && edit.key === location.key ? edit.value : query.text
  const setInput = (value: string) => {
    setEdit({ key: location.key, value })
  }
  const [focused, setFocused] = useState(false)
  const [sheetOpen, setSheetOpen] = useState(false)
  const online = useOnline()
  // The view: the URL when it says so, else the preference (architecture §2.4).
  const view: ViewMode = query.view ?? prefs.viewMode
  // The highlighted option belongs to one debounced text; another text starts from "none".
  const [highlight, setHighlight] = useState<{
    readonly text: string
    readonly index: number
  } | null>(null)
  const inputRef = useRef<HTMLInputElement>(null)
  const listId = useId()
  const debounced = useDebouncedValue(input, DEBOUNCE_MS)
  const active =
    focused && highlight !== null && highlight.text === debounced ? highlight.index : -1
  const setActive = (index: number) => {
    setHighlight({ text: debounced, index })
  }

  const commit = (next: QueryState) => {
    setEdit(null)
    setParams(formatQuery(next))
  }
  const nameOf = useCallback(
    (summary: CardSummary) => displayName(summary.name, uiLanguage, prefs.nameDisplay),
    [uiLanguage, prefs.nameDisplay],
  )

  const suggestions: Suggestion[] = useMemo(
    () => (catalog && debounced !== "" ? catalog.suggest(debounced, edition, SUGGEST_LIMIT) : []),
    [catalog, debounced, edition],
  )
  // Suggestions ignore the list's filters (architecture §2.2: a suggest background is `?q=` only),
  // so "see all" counts and commits the typed text alone. It reads the input, not the debounced
  // text, so a click during the debounce window keeps what was typed last.
  const typed = input.trim()
  // While the debounce is pending the list still shows the previous text's rows; picking one
  // would open a card the new text may not match and rebuild the list URL from the old text.
  // Compared untrimmed on both sides: `typed` is only for what "see all" commits.
  const stale = input !== debounced
  const suggestTotal = useMemo(
    () =>
      catalog && typed !== ""
        ? catalog.results({ ...DEFAULT_QUERY, text: typed }, edition).length
        : 0,
    [catalog, typed, edition],
  )
  const rows: SuggestRow[] = useMemo(() => {
    if (!catalog) return []
    if (debounced !== "") {
      return suggestions.flatMap((item) => {
        const summary = catalog.summary(item.printingId)
        return summary ? [{ summary, name: nameOf(summary), field: item.field }] : []
      })
    }
    return recent.flatMap((printingId) => {
      const summary = catalog.summary(printingId)
      return summary ? [{ summary, name: nameOf(summary) }] : []
    })
  }, [catalog, debounced, suggestions, recent, nameOf])
  const listOpen = focused && catalog !== null

  const results = useMemo(
    () => (catalog ? catalog.results(query, edition) : []),
    [catalog, query, edition],
  )
  const pages = Math.max(1, fixedPages ?? entry.pages ?? 1)
  const visible = useMemo(() => results.slice(0, pages * PAGE_SIZE), [results, pages])
  const cells: GridCell[] = useMemo(() => {
    if (!catalog) return []
    return visible.flatMap((item) => {
      const summary = catalog.summary(item.printingId)
      if (!summary) return []
      const state: CardEntryState = {
        background: search ?? location.search,
        source: "results",
        pages,
        resultKey: item.key,
      }
      return [{ key: item.key, summary, name: nameOf(summary), to: cardPath(summary), state }]
    })
  }, [catalog, visible, location.search, search, pages, nameOf])

  const openSuggestion = (index: number) => {
    const row = rows[index]
    if (!row || stale) return
    // Not stale here, so `typed` is the debounced text trimmed: the list URL never carries spaces.
    const fromSuggest = typed !== ""
    // The typed text becomes the list's URL first, so back returns to it with the string kept.
    const background = fromSuggest ? `?q=${encodeURIComponent(typed)}` : location.search
    if (fromSuggest && background !== location.search) {
      void navigate(location.pathname + background, { replace: true, state: { pages: 1 } })
    }
    const state: CardEntryState = {
      background,
      source: fromSuggest ? "suggest" : "results",
      pages: 1,
      resultKey: row.summary.cardId,
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
  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      if (rows.length === 0) return
      event.preventDefault()
      setFocused(true)
      const step = event.key === "ArrowDown" ? 1 : -1
      setActive((active + step + rows.length) % rows.length)
      return
    }
    if (event.key === "Escape") {
      if (focused) {
        event.preventDefault()
        setFocused(false)
      }
      return
    }
    if (event.key !== "Enter") return
    event.preventDefault()
    // Enter opens the highlighted (else first) suggestion once the list has caught up with the text.
    if (input !== "" && rows.length > 0 && debounced === input) {
      openSuggestion(active === -1 ? 0 : active)
      return
    }
    if (input === "" && active >= 0) {
      openSuggestion(active)
      return
    }
    setFocused(false)
    inputRef.current?.blur()
    commit({ ...query, text: input })
  }
  const openCell = (cell: GridCell) => {
    // The anchor goes on the list's own entry first; the cell then pushes the card entry.
    updateEntry({ anchor: cell.key, pages })
    recentStore.push(cell.summary.printingId)
  }

  const quickBar: QuickBarClass[] = useMemo(
    () =>
      catalog
        ? quickBarClasses(catalog.classCodes).map((code) => ({
            code,
            label:
              code === NEUTRAL_CLASS ? t("search.neutral") : catalog.classLabel(code, textLang),
          }))
        : [],
    [catalog, textLang, t],
  )
  // Returning from a card: the closed dialog restored focus to the old cell only if that element
  // still exists; after a remount the anchor cell takes it so keyboard users continue in place.
  const anchor = entry.anchor
  useEffect(() => {
    if (anchor === undefined || inert || cells.length === 0) return
    const cell = document.querySelector<HTMLElement>(`a[data-result-key="${CSS.escape(anchor)}"]`)
    if (cell && document.activeElement === document.body) cell.focus({ preventScroll: true })
  }, [anchor, inert, cells.length])
  const options = useMemo(() => catalog?.filterOptions(), [catalog])
  const previewIds = useMemo(() => cells.map((cell) => cell.summary.printingId), [cells])
  const preview = useEffectPreviews(client, catalog, previewIds, view === "table")
  const textContext = useTextContext(client, catalog, view === "table")
  const chips = useMemo(
    () =>
      catalog
        ? chipsFor(query, {
            classLabel: (code) => catalog.classLabel(code, textLang),
            neutralLabel: t("search.neutral"),
            typeLabel: (code) => catalog.vocabularyLabel("type", code, textLang),
            rarityLabel: (code) => catalog.vocabularyLabel("rarity", code, textLang),
            costLabel: (range) => t("filters.chip.cost", { range }),
            mechanicLabel: (id, wanted) => {
              const name =
                options?.keywords.find((keyword) => keyword.id === id)?.name(textLang) ?? id
              return wanted === "has"
                ? t("filters.chip.has", { name })
                : t("filters.chip.not", { name })
            },
            altLabel: t("filters.chip.alt"),
            unitLabel: (unit) => t(`filters.units.${unit}`),
          })
        : [],
    [catalog, query, textLang, options, t],
  )
  const setView = (next: ViewMode) => {
    prefsStore.set({ viewMode: next })
    setEdit(null)
    // Switching writes both the URL and the preference; the list's own entry state (loaded pages,
    // anchor) travels to the new entry, so the conditions and the anchor card stay.
    const search = formatQuery({ ...query, view: next }).toString()
    void navigate(location.pathname + (search === "" ? "" : `?${search}`), { state: entry })
  }
  const filterCount = activeFilterCount(query)
  const hasConditions = query.text !== "" || filterCount > 0
  const failed = status.state === "error"
  const loading = catalog === null && !failed

  return (
    <div className="flex flex-col gap-3 pt-2 pb-6" inert={inert}>
      <h1 className="sr-only">{t("pages.cards")}</h1>
      <div className="relative z-20">
        <SearchBar
          value={input}
          onChange={(value) => {
            setInput(value)
            setFocused(true)
          }}
          onKeyDown={onKeyDown}
          onFocus={() => {
            setFocused(true)
          }}
          onBlur={() => {
            setFocused(false)
          }}
          inputRef={inputRef}
          listId={listId}
          expanded={listOpen}
          {...(active >= 0 ? { activeOptionId: optionId(listId, active) } : {})}
          filterCount={filterCount}
          {...(catalog
            ? {
                onOpenFilters: () => {
                  setSheetOpen(true)
                },
              }
            : {})}
        />
        {listOpen && (
          <div className="absolute inset-x-0 top-full mt-1">
            <SuggestList
              id={listId}
              rows={rows}
              activeIndex={active}
              images={images}
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
      {chips.length > 0 && (
        <FilterChips
          chips={chips}
          onChange={commit}
          onClear={() => {
            commit({
              ...DEFAULT_QUERY,
              text: query.text,
              ...(query.view === undefined ? {} : { view: query.view }),
            })
          }}
        />
      )}
      {!online && (
        <p className="rounded-badge bg-warning-soft px-2 py-1 text-12 text-warning">
          {t("views.offline")}
        </p>
      )}
      {catalog && options && sheetOpen && (
        <FilterSheet
          open={sheetOpen}
          onClose={() => {
            setSheetOpen(false)
          }}
          applied={query}
          onApply={(next) => {
            setSheetOpen(false)
            commit(next)
          }}
          options={options}
          classes={quickBar}
          label={(kind, code) => catalog.vocabularyLabel(kind, code, textLang)}
          uiLang={textLang}
          count={(draft) => catalog.results(draft, edition).length}
          coverage={catalog.mechanicCoverageSummary(edition)}
        />
      )}
      {failed && (
        <div
          role="alert"
          className="flex flex-wrap items-center gap-x-3 gap-y-1 rounded-block border border-danger bg-danger-soft px-4 py-3 text-14"
        >
          <span className="font-semibold text-danger">{t("results.loadFailed")}</span>
          <span className="text-text-2">{t("results.loadFailedHint")}</span>
          <Button
            className="ml-auto h-9"
            onClick={() => {
              void client.retry()
            }}
          >
            {t("results.retry")}
          </Button>
        </div>
      )}
      {loading && (
        <>
          <p className="text-13 text-text-3" aria-live="polite">
            {t("results.loading")}
          </p>
          <SkeletonGrid />
        </>
      )}
      {catalog !== null && results.length === 0 && (
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
      {catalog !== null && results.length > 0 && (
        <>
          <ControlBar
            count={results.length}
            unit={query.unit}
            sort={query.sort}
            onSort={(sort) => {
              commit({ ...query, sort })
            }}
            view={view}
            onView={setView}
            uiLanguage={uiLanguage}
            density={prefs.gridDensity}
            onDensity={(gridDensity) => {
              prefsStore.set({ gridDensity })
            }}
          />
          {view === "grid" && (
            <CardGrid
              cells={cells}
              images={images}
              onOpen={openCell}
              density={prefs.gridDensity}
              {...(entry.anchor === undefined ? {} : { anchor: entry.anchor })}
            />
          )}
          {view === "table" && (
            <TableView
              cells={cells}
              images={images}
              onOpen={openCell}
              preview={preview}
              context={textContext}
              uiLang={textLang}
              {...(entry.anchor === undefined ? {} : { anchor: entry.anchor })}
            />
          )}
          {view === "list" && (
            <ListView
              cells={cells}
              onOpen={openCell}
              {...(entry.anchor === undefined ? {} : { anchor: entry.anchor })}
            />
          )}
          {visible.length < results.length && (
            <Button
              className="mx-auto mt-2 w-full max-w-80"
              onClick={() => {
                updateEntry({ pages: pages + 1 })
              }}
            >
              {t("results.loadMore")}
            </Button>
          )}
        </>
      )}
    </div>
  )
}
