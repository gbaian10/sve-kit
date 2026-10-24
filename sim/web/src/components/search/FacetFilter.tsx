import { type ReactNode, useId, useState } from "react"
import { useTranslation } from "react-i18next"

import { normalizeText } from "../../domain/normalize"
import { FilterDropdown } from "./FilterDropdown"

interface FacetOption {
  readonly code: string
  readonly label: string
  readonly fullWidth?: boolean
}

export function FacetCheckbox({
  label,
  content,
  checked,
  indeterminate = false,
  count,
  onChange,
}: {
  readonly label: string
  /** The visual (a badge image) shown in place of the label text; the label stays the name. */
  readonly content?: ReactNode
  readonly checked: boolean
  /** Some of what the row stands for is selected: shown as a dash, announced as mixed. */
  readonly indeterminate?: boolean
  readonly count: number
  readonly onChange: () => void
}) {
  const countId = useId()
  return (
    <label className="flex min-h-11 cursor-pointer items-center gap-2 px-2 text-13 hover:bg-surface-2">
      <input
        type="checkbox"
        checked={checked}
        ref={(input) => {
          if (input) input.indeterminate = indeterminate && !checked
        }}
        aria-checked={indeterminate && !checked ? "mixed" : checked}
        aria-label={label}
        aria-describedby={countId}
        onChange={onChange}
        className="size-4 shrink-0 accent-accent"
      />
      <span className="flex items-center">{content ?? label}</span>
      <span id={countId} className="ml-auto text-text-3 tabular-nums">
        {count}
      </span>
    </label>
  )
}

/** A dropdown checklist of flat values (the set filter); values with sub-values use FacetChips. */
export function FacetFilter({
  label,
  groups,
  counts,
  selected,
  onChange,
  searchable = false,
  quickOptions,
  pinned,
  isChecked,
  onToggle,
  labelOf,
}: {
  readonly label: string
  readonly groups: readonly (readonly FacetOption[])[]
  readonly counts: Readonly<Record<string, number>> | undefined
  readonly selected: readonly string[]
  readonly onChange: (selected: string[]) => void
  /** Adds a text box that narrows the list by label (for long lists such as traits). */
  readonly searchable?: boolean
  readonly quickOptions?: ReactNode
  /** Summary rows above the list; the search box never narrows them. */
  readonly pinned?: ReactNode
  /** Overrides how a list row reads as selected, for selections that stand for many values. */
  readonly isChecked?: (code: string) => boolean
  readonly onToggle?: (code: string) => void
  /** Names a selected token the list does not carry (a reserved choice) in the closed summary. */
  readonly labelOf?: (code: string) => string | undefined
}) {
  const { t } = useTranslation()
  const [filter, setFilter] = useState("")
  const searchId = useId()
  const toggle = (code: string) => {
    onChange(
      selected.includes(code)
        ? selected.filter((value) => value !== code)
        : [...selected, code].sort(),
    )
  }
  // Matching folds width and case like the search bar, so katakana and Latin both narrow by prefix
  // or any inner run; a selected value always stays listed so it can be cleared.
  const needle = normalizeText(filter)
  const matches = (item: FacetOption) => normalizeText(item.label).includes(needle)
  const shown = groups
    .map((items) =>
      items.filter((item) => needle === "" || selected.includes(item.code) || matches(item)),
    )
    .filter((items) => items.length > 0)
  const nothingMatches = needle !== "" && !groups.some((items) => items.some(matches))
  return (
    <FilterDropdown
      label={label}
      disabled={!counts}
      selectedCount={selected.length}
      summary={
        selected.length > 0
          ? selected
              .map(
                (code) =>
                  groups.flat().find((item) => item.code === code)?.label ??
                  labelOf?.(code) ??
                  code,
              )
              .join("、")
          : t("rarityBadge.all")
      }
      onClear={() => {
        onChange([])
      }}
    >
      {quickOptions && (
        <div className="mb-3 flex flex-wrap gap-2 border-b border-border pb-3">{quickOptions}</div>
      )}
      {searchable && (
        <input
          id={searchId}
          type="search"
          value={filter}
          disabled={!counts}
          aria-label={t("filters.searchIn", { label })}
          placeholder={t("filters.searchIn", { label })}
          autoComplete="off"
          onChange={(event) => {
            setFilter(event.target.value)
          }}
          className="mb-2 h-9 w-full rounded-control border border-border bg-surface-1 px-2 text-13 text-text-1 placeholder:text-text-3"
        />
      )}
      <fieldset disabled={!counts}>
        <legend className="sr-only">{label}</legend>
        {pinned}
        {searchable && nothingMatches && (
          <p className="px-2 py-2 text-13 text-text-3">{t("filters.noMatch")}</p>
        )}
        {shown.map((items, index) => (
          <div key={items.at(0)?.code ?? index}>
            {index > 0 && (
              <hr className="my-2 border-0 border-t border-dashed border-border-strong" />
            )}
            <ul className="grid grid-cols-2 gap-x-4">
              {items.map((item) => (
                <li key={item.code} className={item.fullWidth ? "col-span-2" : undefined}>
                  <FacetCheckbox
                    label={item.label}
                    count={counts?.[item.code] ?? 0}
                    checked={isChecked ? isChecked(item.code) : selected.includes(item.code)}
                    onChange={() => {
                      if (onToggle) onToggle(item.code)
                      else toggle(item.code)
                    }}
                  />
                </li>
              ))}
            </ul>
          </div>
        ))}
      </fieldset>
    </FilterDropdown>
  )
}
