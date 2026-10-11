import { type KeyboardEvent, useCallback, useId, useMemo, useRef, useState } from "react"
import { useTranslation } from "react-i18next"
import { useLocation, useNavigate, useSearchParams } from "react-router"

import { type CardEntryState, useListEntryState } from "../../app/listEntryState"
import { useCatalog, useImageIndex, useSearch, useSearchReadiness } from "../../app/snapshot"
import { StatusStrip } from "../../app/StatusStrip"
import { useDebouncedValue } from "../../app/useDebouncedValue"
import { MetadataStatus } from "../../components/card/MetadataStatus"
import { CardGrid, type GridCell } from "../../components/results/CardGrid"
import { CardList, CardTable } from "../../components/results/CardRows"
import { SkeletonGrid } from "../../components/results/SkeletonGrid"
import { ClassQuickBar, type QuickBarClass } from "../../components/search/ClassQuickBar"
import { SearchBar } from "../../components/search/SearchBar"
import { optionId } from "../../components/search/suggest-ids"
import { SuggestList, type SuggestRow } from "../../components/search/SuggestList"
import { Button } from "../../components/ui/Button"
import { Chip } from "../../components/ui/Chip"
import { Notice } from "../../components/ui/Notice"
import { Segmented } from "../../components/ui/Segmented"
import { type CardSummary, type SearchQuery, stringValue } from "../../data"
import { quickBarClasses } from "../../domain/classes"
import { displayName, UI_TEXT_LANG } from "../../domain/nameDisplay"
import { formatQuery, parseQuery } from "../../domain/query/codec"
import {
  activeFilterCount,
  DEFAULT_QUERY,
  NEUTRAL_CLASS,
  type QueryState,
  toggleClass,
  VIEW_MODES,
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

// The search page: the URL holds the query, the input holds what is being typed, and the suggest
// list follows the typed text (debounced) until Enter or "see all" commits it to the URL. The
// result list comes from the search hook; suggestions still read the catalog directly (they need
// the matched field, which the hook's page items do not carry).
export function CardsPage() {
  const { t } = useTranslation()
  const prefs = usePrefs()
  const uiLanguage = currentUiLanguage(prefs.uiLanguage)
  const textLang = UI_TEXT_LANG[uiLanguage]
  const edition = prefs.cardEdition
  const { client, catalog } = useCatalog()
  const readiness = useSearchReadiness(edition)
  const images = useImageIndex(client, catalog !== null)
  const [params, setParams] = useSearchParams()
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

  const pages = Math.max(1, entry.pages ?? 1)
  const searchQuery = useMemo<SearchQuery>(
    () => ({ state: query, edition, offset: 0, limit: pages * PAGE_SIZE }),
    [query, edition, pages],
  )
  const { result } = useSearch(searchQuery)
  // Search opens once the full index is ready, but plain browsing must not wait for it: with no
  // text or filters the worker returns the snapshot order, which the loaded catalog already has.
  const browsing =
    query.text === "" &&
    activeFilterCount(query) === 0 &&
    query.unit === "card" &&
    query.sort === "no"
  const page = useMemo(() => {
    if (result.phase === "complete") return result.page
    if (result.phase === "error" || !browsing || !catalog) return null
    const items = catalog.results(query, edition)
    return {
      total: items.length,
      offset: 0,
      items: items.slice(0, pages * PAGE_SIZE).map((item) => ({
        key: item.key,
        printingId: item.printingId,
        summary: catalog.summary(item.printingId),
      })),
    }
  }, [result, browsing, catalog, query, edition, pages])
  const cells: GridCell[] = useMemo(() => {
    if (!page) return []
    return page.items.flatMap((item) => {
      if (!item.summary) return []
      const state: CardEntryState = {
        background: location.search,
        source: "results",
        pages,
        resultKey: item.key,
      }
      return [
        {
          key: item.key,
          summary: item.summary,
          name: nameOf(item.summary),
          to: cardPath(item.summary),
          state,
        },
      ]
    })
  }, [page, location.search, pages, nameOf])

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
  const classLabel = (code: string) => quickBar.find((item) => item.code === code)?.label ?? code
  const filterCount = activeFilterCount(query)
  const hasConditions = query.text !== "" || filterCount > 0
  const view = query.view ?? prefs.viewMode
  const unsupported = result.phase === "error" && result.error.kind === "unsupported-query"
  const pending = result.phase === "pending" && page === null
  const effectOwner = useCallback(
    (cell: GridCell) => {
      const revision = catalog?.index.displayRevision(cell.summary.faceId, cell.summary.region)
      return revision ? { kind: "face_revision", id: stringValue(revision["id"]) } : null
    },
    [catalog],
  )

  return (
    <div className="flex flex-col gap-3 pt-2 pb-6">
      <h1 className="sr-only">{t("pages.cards")}</h1>
      <StatusStrip readiness={readiness} />
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
      {catalog !== null && <MetadataStatus client={client} />}
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
            facets: (result.error.facets ?? [])
              .map((facet) => t(`results.facet.${facet}`))
              .join("、"),
          })}
        </Notice>
      )}
      {/* The status strip above says what is downloading; the list only shows its skeleton. */}
      {pending && readiness.phase !== "error" && <SkeletonGrid />}
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
              {t("results.countUnit")}
            </p>
            <div className="ml-auto flex items-center gap-2">
              {/* Sorting arrives with the filter panel (V3); the control keeps its place and says so. */}
              <button
                type="button"
                aria-disabled="true"
                title={t("results.sortSoon")}
                className="flex h-8 items-center gap-1 rounded-control border border-border bg-surface-1 px-2.5 text-13 text-text-3"
              >
                <span>{t("results.sort")}</span>
                <span className="text-text-2">{t("results.sortDefault")}</span>
              </button>
              <Segmented
                size="sm"
                tone="accent"
                label={t("results.view")}
                options={VIEW_MODES.map((value) => ({ value, label: t(`options.view.${value}`) }))}
                value={view}
                onChange={(next) => {
                  prefsStore.set({ viewMode: next })
                  commit({ ...query, view: next })
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
              client={client}
              effectOwner={effectOwner}
              {...(entry.anchor === undefined ? {} : { anchor: entry.anchor })}
            />
          )}
          {view === "list" && (
            <CardList
              cells={cells}
              onOpen={openCell}
              {...(entry.anchor === undefined ? {} : { anchor: entry.anchor })}
            />
          )}
          {page.items.length < page.total && (
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
