import { type ChangeEvent, type KeyboardEvent, type RefObject } from "react"
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
  /** Facets set beyond the text; 0 hides the badge. */
  readonly filterCount: number
  readonly onOpenFilters?: () => void
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
  onOpenFilters,
}: SearchBarProps) {
  const { t } = useTranslation()
  return (
    <div className="flex items-center gap-2">
      <div className="relative flex h-11 min-w-0 flex-1 items-center">
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
      <button
        type="button"
        aria-label={t("search.filters")}
        title={onOpenFilters ? t("search.filters") : t("search.filtersSoon")}
        aria-disabled={onOpenFilters === undefined}
        onClick={onOpenFilters}
        className={cn(
          "relative flex size-11 shrink-0 items-center justify-center rounded-button border border-border bg-surface-1 text-text-1",
          onOpenFilters ? "hover:bg-surface-2" : "text-text-3",
        )}
      >
        <svg
          aria-hidden="true"
          viewBox="0 0 24 24"
          className="size-5"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
        >
          <path d="M4 6h16M7 12h10M10 18h4" />
        </svg>
        {filterCount > 0 && (
          <span className="absolute top-1 right-1 flex h-4.5 min-w-4.5 items-center justify-center rounded-full bg-accent px-1 text-11 font-bold text-accent-ink tabular-nums">
            {filterCount}
          </span>
        )}
      </button>
    </div>
  )
}
