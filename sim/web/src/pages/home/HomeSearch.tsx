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

import type { CardEntryState } from "../../app/listEntryState"
import { useSearch } from "../../app/snapshot"
import { useDebouncedValue } from "../../app/useDebouncedValue"
import logo from "../../assets/official/logo/head-left.png"
import { SearchBar } from "../../components/search/SearchBar"
import { optionId } from "../../components/search/suggest-ids"
import { SuggestList, type SuggestRow } from "../../components/search/SuggestList"
import type { Catalog, ImageIndex, SearchQuery } from "../../data"
import type { DisplayedName } from "../../domain/nameDisplay"
import { querySearch } from "../../domain/query/codec"
import { DEFAULT_QUERY } from "../../domain/query/model"
import type { Region } from "../../domain/search"
import { recentStore } from "../../settings"
import { cardPath, homeEntryState, readHomeEntry } from "./home-model"

const SUGGEST_LIMIT = 8
const DEBOUNCE_MS = 150
const SHORTCUT =
  "flex h-10 min-w-0 items-center justify-center gap-1.5 rounded-button border border-border bg-surface-1 px-3 text-14 font-medium text-text-1 hover:bg-surface-2 lg:px-4"

export interface HomeSearchProps {
  /** Null until the selected region's basic catalog is complete. */
  readonly catalog: Catalog | null
  readonly failed: boolean
  readonly edition: Region
  readonly images: ImageIndex | undefined
  /** Recently viewed rows; null while the catalog that resolves them is not loaded. */
  readonly recent: readonly SuggestRow[] | null
  readonly nameOf: (summary: SuggestRow["summary"]) => DisplayedName
}

export function HomeSearch({ catalog, failed, edition, images, recent, nameOf }: HomeSearchProps) {
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
  const query = useMemo<SearchQuery | null>(
    () =>
      settled === ""
        ? null
        : { state: { ...DEFAULT_QUERY, text: settled }, edition, offset: 0, limit: 0 },
    [settled, edition],
  )
  const { lastInput, result } = useSearch(query)
  const complete =
    catalog !== null &&
    settled !== "" &&
    result.phase === "complete" &&
    lastInput?.state.text === settled &&
    lastInput.edition === edition
  const rows: readonly SuggestRow[] = useMemo(() => {
    if (settled === "") return recent ?? []
    if (!complete) return []
    return catalog.suggest(settled, edition, SUGGEST_LIMIT).flatMap((item) => {
      const summary = catalog.summary(item.printingId)
      return summary ? [{ summary, name: nameOf(summary), field: item.field }] : []
    })
  }, [settled, recent, complete, catalog, edition, nameOf])
  const total = complete ? result.page.total : 0
  const stale = input !== debounced
  const active =
    focused && highlight !== null && highlight.text === debounced ? highlight.index : -1
  const ready = catalog !== null
  const listOpen = focused && ready && !failed

  const remember = () => {
    void navigate(location.pathname, { replace: true, state: homeEntryState(input) })
  }
  const openRow = (index: number) => {
    const row = rows[index]
    if (!row || stale) return
    const state: CardEntryState =
      settled === ""
        ? { background: "", source: "results", pages: 1, resultKey: row.summary.cardId }
        : {
            background: querySearch({ ...DEFAULT_QUERY, text: settled }),
            source: "suggest",
            pages: 1,
            resultKey: row.summary.cardId,
          }
    recentStore.push(row.summary.printingId)
    setFocused(false)
    remember()
    void navigate(cardPath(row.summary), { state })
  }
  const seeAll = () => {
    setFocused(false)
    remember()
    void navigate(`/cards${querySearch({ ...DEFAULT_QUERY, text: typed })}`)
  }
  const activate = (index: number) => {
    if (rows.length > 0) openRow(index === -1 ? 0 : index)
    else seeAll()
  }
  const openArmed = useEffectEvent(() => {
    if (armed.current === null || armed.current !== debounced || !complete) return
    armed.current = null
    activate(-1)
  })
  useEffect(() => {
    openArmed()
  }, [debounced, complete])

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
    if (!ready || failed) {
      setFocused(true)
      return
    }
    if (stale || !complete) {
      armed.current = input
      return
    }
    activate(active)
  }

  const pickRandom = () => {
    if (!catalog) return
    const pool = catalog.entries.filter((entry) =>
      entry.printings.some((printing) => printing.region === edition),
    )
    const entry = pool[Math.floor(Math.random() * pool.length)]
    if (!entry) return
    const printingId =
      entry.defaultPrinting[edition] ??
      entry.printings.find((printing) => printing.region === edition)?.id
    const summary = printingId === undefined ? undefined : catalog.summary(printingId)
    if (!summary) return
    recentStore.push(summary.printingId)
    remember()
    const state: CardEntryState = {
      background: "",
      source: "results",
      pages: 1,
      resultKey: entry.cardId,
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
        {focused && !listOpen && (
          <div className="absolute inset-x-0 top-full mt-1">
            <p
              role="status"
              className="rounded-block border border-border bg-surface-1 px-4 py-3 text-13 leading-relaxed text-text-2 shadow-lg"
            >
              {failed ? t("home.searchFailed") : t("home.searchPending")}
            </p>
          </div>
        )}
        {listOpen && (
          <div className="absolute inset-x-0 top-full mt-1">
            <SuggestList
              id={listId}
              rows={rows}
              activeIndex={active}
              images={images}
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
        <RouterLink to="/cards" className={SHORTCUT}>
          <SlidersHorizontal className="size-4 shrink-0" aria-hidden="true" />
          <span className="truncate">{t("home.shortcut.advanced")}</span>
        </RouterLink>
        <RouterLink to="/sets" className={SHORTCUT}>
          <Layers className="size-4 shrink-0" aria-hidden="true" />
          <span className="truncate">{t("home.shortcut.sets")}</span>
        </RouterLink>
        <button
          type="button"
          disabled={!ready || failed}
          title={ready ? undefined : t("home.randomPending")}
          onClick={pickRandom}
          className={`${SHORTCUT} disabled:cursor-not-allowed disabled:text-text-3 disabled:hover:bg-surface-1`}
        >
          <Shuffle className="size-4 shrink-0" aria-hidden="true" />
          <span className="truncate">{t("home.shortcut.random")}</span>
        </button>
      </nav>
    </section>
  )
}
