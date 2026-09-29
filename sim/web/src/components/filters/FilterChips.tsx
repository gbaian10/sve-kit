import { useTranslation } from "react-i18next"

import type { QueryState } from "../../domain/query/model"
import type { FilterChip } from "./chips"

export type { FilterChip } from "./chips"

const VISIBLE = 4

/** The applied facets as 28px chips with a clear link (design §6.2); each chip removes itself. */
export function FilterChips({
  chips,
  onChange,
  onClear,
}: {
  readonly chips: readonly FilterChip[]
  readonly onChange: (next: QueryState) => void
  readonly onClear: () => void
}) {
  const { t } = useTranslation()
  if (chips.length === 0) return null
  const shown = chips.slice(0, VISIBLE)
  const rest = chips.length - shown.length
  return (
    <div className="flex flex-wrap items-center gap-1.5">
      {shown.map((chip) => (
        <button
          key={chip.key}
          type="button"
          aria-label={t("filters.remove", { label: chip.label })}
          onClick={() => {
            onChange(chip.remove())
          }}
          className="flex h-7 items-center gap-1 rounded-pill bg-surface-2 px-2.5 text-12 font-semibold text-text-1 hover:bg-surface-3"
        >
          <span>{chip.label}</span>
          <span aria-hidden="true" className="text-text-3">
            ×
          </span>
        </button>
      ))}
      {rest > 0 && (
        <span className="text-12 text-text-3">{t("filters.more", { count: rest })}</span>
      )}
      <button
        type="button"
        onClick={onClear}
        className="ml-1 text-12 text-accent-text hover:underline"
      >
        {t("filters.clear")}
      </button>
    </div>
  )
}
