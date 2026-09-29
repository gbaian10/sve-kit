import { type KeyboardEvent, useCallback, useId, useMemo, useRef, useState } from "react"
import { useTranslation } from "react-i18next"
import { useLocation, useNavigate, useSearchParams } from "react-router"

import { type CardEntryState, useListEntryState } from "../../app/listEntryState"
import { useCatalog, useImageIndex } from "../../app/snapshot"
import { useDebouncedValue } from "../../app/useDebouncedValue"
import { CardGrid, type GridCell } from "../../components/results/CardGrid"
import { SkeletonGrid } from "../../components/results/SkeletonGrid"
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
} from "../../domain/query/model"
import type { Suggestion } from "../../domain/search"
import { currentUiLanguage } from "../../i18n"
import { recentStore, usePrefs, useRecent } from "../../settings"

const PAGE_SIZE = 60
const SUGGEST_LIMIT = 8
const DEBOUNCE_MS = 150

function cardPath(summary: CardSummary): string {
  return `/cards/${encodeURIComponent(summary.cardNo)}`
}

// The search page: the URL holds the query, the input holds what is being typed, and the suggest
// list follows the typed text (debounced) until Enter or "see all" commits it to the URL.
export function CardsPage() {
  const { t } = useTranslation()
  const prefs = usePrefs()
  const uiLanguage = currentUiLanguage(prefs.uiLanguage)
  const textLang = UI_TEXT_LANG[uiLanguage]
  const edition = prefs.cardEdition
  const { client, status, catalog } = useCatalog()
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
  const suggestTotal = useMemo(
    () =>
      catalog && debounced !== ""
        ? catalog.results({ ...query, text: debounced }, edition).length
        : 0,
    [catalog, debounced, edition, query],
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
  const pages = Math.max(1, entry.pages ?? 1)
  const visible = useMemo(() => results.slice(0, pages * PAGE_SIZE), [results, pages])
  const cells: GridCell[] = useMemo(() => {
    if (!catalog) return []
    return visible.flatMap((item) => {
      const summary = catalog.summary(item.printingId)
      if (!summary) return []
      const state: CardEntryState = {
        background: location.search,
        source: "results",
        pages,
        resultKey: item.key,
      }
      return [{ key: item.key, summary, name: nameOf(summary), to: cardPath(summary), state }]
    })
  }, [catalog, visible, location.search, pages, nameOf])

  const openSuggestion = (index: number) => {
    const row = rows[index]
    if (!row) return
    const fromSuggest = debounced !== ""
    // The typed text becomes the list's URL first, so back returns to it with the string kept.
    const background = fromSuggest ? `?q=${encodeURIComponent(debounced)}` : location.search
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
    commit({ ...query, text: debounced })
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
  const filterCount = activeFilterCount(query)
  const hasConditions = query.text !== "" || filterCount > 0
  const failed = status.state === "error"
  const loading = catalog === null && !failed

  return (
    <div className="flex flex-col gap-3 pt-2 pb-6">
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
              {...(debounced !== ""
                ? { query: { text: debounced, total: suggestTotal, onSeeAll: seeAll } }
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
          <p className="text-13 text-text-2" aria-live="polite">
            <b className="font-semibold text-text-1 tabular-nums">
              {new Intl.NumberFormat(uiLanguage).format(results.length)}
            </b>{" "}
            {t("results.countUnit")}
          </p>
          <CardGrid
            cells={cells}
            images={images}
            onOpen={openCell}
            {...(entry.anchor === undefined ? {} : { anchor: entry.anchor })}
          />
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
