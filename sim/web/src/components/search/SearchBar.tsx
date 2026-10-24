import { type ChangeEvent, type KeyboardEvent, type ReactNode, type RefObject } from "react"
import { useTranslation } from "react-i18next"

import { cn } from "../ui/cn"

export interface SearchBarProps {
  readonly value: string
  readonly onChange: (value: string) => void
  readonly onKeyDown: (event: KeyboardEvent<HTMLInputElement>) => void
  readonly onFocus: () => void
  readonly onBlur: () => void
  readonly inputRef: RefObject<HTMLInputElement | null>
  /** Id of the suggest listbox; the input is a combobox over it. */
  readonly listId: string
  readonly expanded: boolean
  readonly activeOptionId?: string
  /** Facets set beyond the text; 0 hides the badge, undefined hides the filter button. */
  readonly filterCount?: number
  /** The filter panel this button expands; `filtersId` is the panel element's id. */
  readonly filtersOpen?: boolean
  readonly filtersId?: string
  readonly onToggleFilters?: () => void
  /** Rendered after the filter button: the current result count. */
  readonly summary?: ReactNode
}

// Design 04: 44px input, radius 12, surface-1 with a 1px border; the filter button beside it.
export function SearchBar({
  value,
  onChange,
  onKeyDown,
  onFocus,
  onBlur,
  inputRef,
  listId,
  expanded,
  activeOptionId,
  filterCount,
  filtersOpen = false,
  filtersId,
  onToggleFilters,
  summary,
}: SearchBarProps) {
  const { t } = useTranslation()
  const filtersActive = filtersOpen || (filterCount ?? 0) > 0
  return (
    <div className="flex flex-wrap items-center gap-2">
      <div className="relative flex h-11 min-w-24 flex-1 basis-0 items-center">
        <svg
          aria-hidden="true"
          viewBox="0 0 24 24"
          className="pointer-events-none absolute left-3 size-5 text-text-3"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
        >
          <circle cx="11" cy="11" r="7" />
          <path d="m20 20-3.5-3.5" />
        </svg>
        <input
          ref={inputRef}
          type="search"
          role="combobox"
          aria-label={t("search.label")}
          aria-expanded={expanded}
          aria-controls={listId}
          aria-autocomplete="list"
          aria-activedescendant={activeOptionId}
          autoComplete="off"
          autoCorrect="off"
          spellCheck={false}
          enterKeyHint="search"
          placeholder={t("search.placeholder")}
          value={value}
          onChange={(event: ChangeEvent<HTMLInputElement>) => {
            onChange(event.target.value)
          }}
          onKeyDown={onKeyDown}
          onFocus={onFocus}
          onBlur={onBlur}
          className={cn(
            "h-11 w-full rounded-button border border-border bg-surface-1 pr-10 pl-10 text-16 text-text-1",
            "placeholder:text-text-3 focus:border-border-strong focus:outline-none focus-visible:outline-2",
            "[&::-webkit-search-cancel-button]:hidden",
          )}
        />
        {value !== "" && (
          <button
            type="button"
            aria-label={t("search.clear")}
            onMouseDown={(event) => {
              event.preventDefault()
            }}
            onClick={() => {
              onChange("")
              inputRef.current?.focus()
            }}
            className="absolute right-1 flex size-9 items-center justify-center rounded-control text-text-2 hover:bg-surface-2"
          >
            <svg
              aria-hidden="true"
              viewBox="0 0 24 24"
              className="size-4"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
            >
              <path d="M6 6l12 12M18 6 6 18" />
            </svg>
          </button>
        )}
      </div>
      {filterCount !== undefined && (
        <button
          type="button"
          aria-label={t("search.filters")}
          title={t("search.filters")}
          aria-expanded={filtersOpen}
          aria-controls={filtersId}
          aria-describedby={
            filterCount > 0 && filtersId ? `${filtersId}-count-description` : undefined
          }
          onClick={onToggleFilters}
          className={cn(
            "flex h-11 shrink-0 items-center justify-center gap-2 rounded-button border px-3 text-13 font-semibold whitespace-nowrap",
            filtersActive
              ? "border-accent bg-accent text-accent-ink hover:brightness-105"
              : "border-border bg-surface-1 text-text-1 hover:bg-surface-2",
          )}
        >
          <svg
            aria-hidden="true"
            viewBox="0 0 24 24"
            className="size-4 shrink-0"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
          >
            <path d="M4 6h16M7 12h10M10 18h4" />
          </svg>
          <span>{t("search.filters")}</span>
          {filterCount > 0 && (
            // In flow after the label: a corner badge either covered the text or overhung the
            // button, which the overflow audit reports. The colours invert with the button's.
            <span
              aria-hidden="true"
              className={cn(
                "flex h-5 min-w-5 items-center justify-center rounded-full px-1.5 text-11 font-bold tabular-nums",
                filtersActive ? "bg-accent-ink text-accent" : "bg-accent text-accent-ink",
              )}
            >
              {filterCount}
            </span>
          )}
          {filterCount > 0 && filtersId && (
            <span id={`${filtersId}-count-description`} className="sr-only">
              {t("search.filtersSelected", { count: filterCount })}
            </span>
          )}
        </button>
      )}
      {summary}
    </div>
  )
}
