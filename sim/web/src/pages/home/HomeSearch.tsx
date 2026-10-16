import { Layers, Shuffle, SlidersHorizontal } from "lucide-react"
import {
  type KeyboardEvent,
  useEffect,
  useEffectEvent,
  useId,
  useMemo,
  useRef,
  useState,
} from "react"
import { useTranslation } from "react-i18next"
import { Link as RouterLink, useLocation, useNavigate } from "react-router"

import { type CardEntryState, routeState } from "../../app/listEntryState"
import { useSearch, useWorkerSuggestions } from "../../app/snapshot"
import { useDebouncedValue } from "../../app/useDebouncedValue"
import logo from "../../assets/official/logo/head-left.png"
import { SearchBar } from "../../components/search/SearchBar"
import { optionId } from "../../components/search/suggest-ids"
import { SuggestList, type SuggestRow } from "../../components/search/SuggestList"
import { Button } from "../../components/ui/Button"
import type { ImageIndex, SearchQuery } from "../../data"
import type { DisplayedName } from "../../domain/nameDisplay"
import { editionUrl, querySearch } from "../../domain/query/codec"
import { DEFAULT_QUERY } from "../../domain/query/model"
import type { Region } from "../../domain/search"
import { recentStore } from "../../settings"
import { cardPath, homeEntryState, readHomeEntry } from "./home-model"

const DEBOUNCE_MS = 150
const SHORTCUT =
  "flex h-10 min-w-0 items-center justify-center gap-1.5 rounded-button border border-border bg-surface-1 px-3 text-14 font-medium text-text-1 hover:bg-surface-2 lg:px-4"

export interface HomeSearchProps {
  /** The selected edition's search index is loaded. */
  readonly ready: boolean
  readonly failed: boolean
  readonly onRetry: () => void
  readonly edition: Region
  /** Recently viewed rows; null while the search index that resolves them is not loaded. */
  readonly recent: readonly SuggestRow[] | null
  readonly recentImages: ImageIndex | undefined
  readonly nameOf: (summary: SuggestRow["summary"]) => DisplayedName
}

export function HomeSearch({
  ready,
  failed,
  onRetry,
  edition,
  recent,
  recentImages,
  nameOf,
}: HomeSearchProps) {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const location = useLocation()
  // Back from a card or the list returns here with what was typed.
  const [input, setInput] = useState(() => readHomeEntry(location.state).text)
  const [focused, setFocused] = useState(false)
  const [highlight, setHighlight] = useState<{
    readonly text: string
    readonly index: number
  } | null>(null)
  // Enter during the debounce window opens the first suggestion once the list has caught up.
  const armed = useRef<string | null>(null)
  const inputRef = useRef<HTMLInputElement>(null)
  const listId = useId()
  const debounced = useDebouncedValue(input, DEBOUNCE_MS)
  const typed = input.trim()
  const settled = debounced.trim()
  const suggestion = useWorkerSuggestions(settled, edition)
  const complete =
    ready &&
    settled !== "" &&
    suggestion.result.phase === "complete" &&
    suggestion.lastInput?.state.text === settled &&
    suggestion.lastInput.edition === edition
  const queryFailed =
    ready &&
    settled !== "" &&
    suggestion.result.phase === "error" &&
    suggestion.lastInput?.state.text === settled &&
    suggestion.lastInput.edition === edition
  // Until the next answer arrives the last one stays on screen, dimmed.
  const shown = suggestion.result.phase === "complete" ? suggestion.result : suggestion.previous
  const rows: readonly SuggestRow[] = useMemo(
    () =>
      settled === ""
        ? (recent ?? [])
        : (shown?.suggestions ?? []).map((item) => ({
            summary: item.summary,
            name: nameOf(item.summary),
            field: item.field,
          })),
    [settled, recent, shown, nameOf],
  )
  const suggestionImages = suggestion.images
  const total = complete ? suggestion.result.page.total : 0
  const stale = input !== debounced || (settled !== "" && !complete)
  const active =
    focused && highlight !== null && highlight.text === debounced ? highlight.index : -1
  // An answer without rows stays closed while the next one is on its way, so no false "no match".
  const listOpen =
    focused && ready && !failed && !queryFailed && (settled === "" || complete || rows.length > 0)

  const everyCard = useMemo<SearchQuery>(
    () => ({ state: DEFAULT_QUERY, edition, limit: 1 }),
    [edition],
  )
  const all = useSearch(everyCard)
  const cardCount =
    ready && all.result.phase === "complete" && all.lastInput?.edition === edition
      ? all.result.page.total
      : 0
  // The random card is picked ahead of the click, so the click opens it at once.
  const [seed] = useState(Math.random)
  const pickQuery = useMemo<SearchQuery | null>(
    () =>
      cardCount === 0
        ? null
        : { state: DEFAULT_QUERY, edition, offset: Math.floor(seed * cardCount), limit: 1 },
    [cardCount, edition, seed],
  )
  const picked = useSearch(pickQuery)
  const randomItem =
    pickQuery !== null &&
    picked.result.phase === "complete" &&
    picked.lastInput !== null &&
    picked.lastInput.offset === pickQuery.offset &&
    picked.lastInput.edition === edition
      ? picked.result.page.items[0]
      : undefined

  const remember = () => {
    void navigate(location.pathname, { replace: true, state: homeEntryState(input) })
  }
  const openRow = (index: number) => {
    const row = rows[index]
    if (!row || stale) return
    const state: CardEntryState =
      settled === ""
        ? {
            background: editionUrl("", edition),
            source: "results",
            pages: 1,
            resultKey: row.summary.cardId,
          }
        : {
            background: editionUrl(querySearch({ ...DEFAULT_QUERY, text: settled }), edition),
            source: "suggest",
            pages: 1,
            resultKey: row.summary.cardId,
          }
    recentStore.push(row.summary.printingId)
    setFocused(false)
    remember()
    void navigate(cardPath(row.summary), { state: { ...state, ...routeState(row.summary) } })
  }
  const seeAll = () => {
    setFocused(false)
    remember()
    void navigate(editionUrl(`/cards${querySearch({ ...DEFAULT_QUERY, text: typed })}`, edition))
  }
  const activate = (index: number) => {
    if (rows.length > 0) openRow(index === -1 ? 0 : index)
    else seeAll()
  }
  const openArmed = useEffectEvent(() => {
    if (queryFailed) {
      armed.current = null
      return
    }
    if (armed.current === null || armed.current !== debounced || !complete) return
    armed.current = null
    activate(-1)
  })
  useEffect(() => {
    openArmed()
  }, [debounced, complete, queryFailed])

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.nativeEvent.isComposing) return
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      if (rows.length === 0) return
      event.preventDefault()
      setFocused(true)
      const step = event.key === "ArrowDown" ? 1 : -1
      setHighlight({ text: debounced, index: (active + step + rows.length) % rows.length })
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
    if (typed === "") {
      if (active >= 0) openRow(active)
      return
    }
    // Downloading or failed: keep the text; the panel below the input says why nothing opens.
    if (!ready || failed || queryFailed) {
      setFocused(true)
      return
    }
    if (stale || !complete) {
      armed.current = input
      return
    }
    activate(active)
  }

  const openRandom = () => {
    const summary = randomItem?.summary
    if (!randomItem || !summary) return
    recentStore.push(summary.printingId)
    remember()
    const state: CardEntryState = {
      background: "",
      source: "results",
      pages: 1,
      resultKey: randomItem.key,
      ...routeState(summary),
    }
    void navigate(cardPath(summary), { state })
  }

  return (
    <section
      aria-label={t("search.label")}
      className="mx-auto flex w-full max-w-150 flex-col items-center gap-3"
    >
      <img src={logo} alt="" className="hidden size-22 rounded-dialog object-cover lg:block" />
      <div className="relative z-20 w-full">
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
            remember()
          }}
          inputRef={inputRef}
          listId={listId}
          expanded={listOpen}
          {...(listOpen && active >= 0 ? { activeOptionId: optionId(listId, active) } : {})}
        />
        {(queryFailed ||
          (focused && (!ready || failed || (settled.startsWith("effect:") && !complete)))) && (
          <div className="absolute inset-x-0 top-full mt-1">
            <div
              role={queryFailed ? "alert" : "status"}
              className="rounded-block border border-border bg-surface-1 px-4 py-3 text-13 leading-relaxed text-text-2 shadow-lg"
            >
              <p>{failed || queryFailed ? t("home.searchFailed") : t("home.searchPending")}</p>
              {queryFailed && (
                <Button
                  className="mt-2 h-9"
                  onMouseDown={(event) => {
                    event.preventDefault()
                  }}
                  onClick={() => {
                    inputRef.current?.focus()
                    onRetry()
                  }}
                >
                  {t("status.retry")}
                </Button>
              )}
            </div>
          </div>
        )}
        {listOpen && (
          <div className="absolute inset-x-0 top-full mt-1">
            <SuggestList
              id={listId}
              rows={rows}
              activeIndex={active}
              images={settled === "" ? recentImages : suggestionImages}
              onPick={openRow}
              onHover={(index) => {
                setHighlight({ text: debounced, index })
              }}
              stale={stale}
              {...(settled !== ""
                ? { query: { text: typed, total, onSeeAll: seeAll } }
                : {
                    onClearRecent: () => {
                      recentStore.clear()
                    },
                  })}
            />
          </div>
        )}
      </div>
      <nav
        aria-label={t("home.shortcuts")}
        className="grid w-full grid-cols-3 gap-2 lg:flex lg:w-auto"
      >
        <RouterLink to={editionUrl("/cards", edition)} className={SHORTCUT}>
          <SlidersHorizontal className="size-4 shrink-0" aria-hidden="true" />
          <span className="truncate">{t("home.shortcut.advanced")}</span>
        </RouterLink>
        <RouterLink to={editionUrl("/sets", edition)} className={SHORTCUT}>
          <Layers className="size-4 shrink-0" aria-hidden="true" />
          <span className="truncate">{t("home.shortcut.sets")}</span>
        </RouterLink>
        <button
          type="button"
          disabled={randomItem?.summary === undefined || failed}
          title={randomItem?.summary === undefined ? t("home.randomPending") : undefined}
          onClick={openRandom}
          className={`${SHORTCUT} disabled:cursor-not-allowed disabled:text-text-3 disabled:hover:bg-surface-1`}
        >
          <Shuffle className="size-4 shrink-0" aria-hidden="true" />
          <span className="truncate">{t("home.shortcut.random")}</span>
        </button>
      </nav>
    </section>
  )
}
