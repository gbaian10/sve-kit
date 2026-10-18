import { ChevronDown } from "lucide-react"
import { type ReactNode, useId, useState } from "react"
import { useTranslation } from "react-i18next"

import { normalizeText } from "../../domain/normalize"
import { Button } from "../ui/Button"

interface FacetOption {
  readonly code: string
  readonly label: string
  readonly fullWidth?: boolean
}

export function FacetCheckbox({
  label,
  content,
  checked,
  count,
  onChange,
}: {
  readonly label: string
  /** The visual (a badge image) shown in place of the label text; the label stays the name. */
  readonly content?: ReactNode
  readonly checked: boolean
  readonly count: number
  readonly onChange: () => void
}) {
  const countId = useId()
  return (
    <label className="flex min-h-11 cursor-pointer items-center gap-2 px-2 text-13 hover:bg-surface-2">
      <input
        type="checkbox"
        checked={checked}
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
}: {
  readonly label: string
  readonly groups: readonly (readonly FacetOption[])[]
  readonly counts: Readonly<Record<string, number>> | undefined
  readonly selected: readonly string[]
  readonly onChange: (selected: string[]) => void
  /** Adds a text box that narrows the list by label (for long lists such as traits). */
  readonly searchable?: boolean
}) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const [filter, setFilter] = useState("")
  const id = useId()
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
    <div>
      <Button
        size="sm"
        disabled={!counts}
        aria-expanded={open}
        aria-controls={id}
        onClick={() => {
          setOpen(!open)
        }}
      >
        {label}
        <span className="font-normal text-text-2">
          {selected.length
            ? t("rarityBadge.selected", { count: selected.length })
            : t("rarityBadge.all")}
        </span>
        <ChevronDown aria-hidden="true" className="size-4" />
      </Button>
      <div
        id={id}
        hidden={!open}
        className="mt-2 max-h-96 w-full max-w-xl overflow-y-auto rounded-control border border-border bg-surface-1 p-2"
      >
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
                      checked={selected.includes(item.code)}
                      onChange={() => {
                        toggle(item.code)
                      }}
                    />
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </fieldset>
      </div>
    </div>
  )
}
