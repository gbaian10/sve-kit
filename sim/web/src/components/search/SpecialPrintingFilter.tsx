import { useId } from "react"
import { useTranslation } from "react-i18next"

import type { QueryState } from "../../domain/query/model"
import { FilterDropdown } from "./FilterDropdown"

/** One dropdown for the printing marks; ticking several keeps only cards that have all of them. */
export function SpecialPrintingFilter({
  query,
  onChange,
}: {
  readonly query: QueryState
  readonly onChange: (next: QueryState) => void
}) {
  const { t } = useTranslation()
  const id = useId()
  const options = [
    { key: "stampOnly", label: t("filters.stampOnly"), hint: t("filters.stampOnlyHint") },
    { key: "serialOnly", label: t("filters.serialOnly"), hint: t("filters.serialOnlyHint") },
    { key: "unlistedOnly", label: t("filters.unlistedOnly"), hint: t("filters.unlistedOnlyHint") },
  ] as const
  const selected = options.filter((option) => query[option.key])
  return (
    <FilterDropdown
      label={t("filters.specialPrintings")}
      selectedCount={selected.length}
      summary={
        selected.length > 0
          ? selected.map((option) => option.label).join("、")
          : t("filters.specialPrintingsAll")
      }
      onClear={() => {
        onChange({ ...query, stampOnly: false, serialOnly: false, unlistedOnly: false })
      }}
    >
      <fieldset>
        <legend className="sr-only">{t("filters.specialPrintings")}</legend>
        <ul>
          {options.map((option) => (
            <li key={option.key}>
              <label className="flex min-h-11 cursor-pointer items-start gap-2 px-2 py-2 hover:bg-surface-2">
                <input
                  type="checkbox"
                  checked={query[option.key]}
                  aria-label={option.label}
                  aria-describedby={`${id}-${option.key}`}
                  onChange={(event) => {
                    onChange({ ...query, [option.key]: event.target.checked })
                  }}
                  className="mt-0.5 size-4 shrink-0 accent-accent"
                />
                <span className="min-w-0">
                  <span className="block text-13 text-text-1">{option.label}</span>
                  <span id={`${id}-${option.key}`} className="block text-12 text-text-3">
                    {option.hint}
                  </span>
                </span>
              </label>
            </li>
          ))}
        </ul>
        <p className="mt-2 border-t border-border px-2 pt-2 text-12 text-text-3">
          {t("filters.specialPrintingsHint")}
        </p>
      </fieldset>
    </FilterDropdown>
  )
}
