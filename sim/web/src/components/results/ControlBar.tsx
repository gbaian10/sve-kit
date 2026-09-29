import { useTranslation } from "react-i18next"

import {
  QUERY_SORTS,
  type QuerySort,
  type QueryUnit,
  VIEW_MODES,
  type ViewMode,
} from "../../domain/query/model"
import type { UiLanguage } from "../../i18n/languages"
import { Segmented } from "../ui/Segmented"

export interface ControlBarProps {
  readonly count: number
  readonly unit: QueryUnit
  readonly sort: QuerySort
  readonly onSort: (sort: QuerySort) => void
  readonly view: ViewMode
  readonly onView: (view: ViewMode) => void
  readonly uiLanguage: UiLanguage
  /** Grid density on phones (2 or 3 columns); only offered for the grid view. */
  readonly density: 2 | 3
  readonly onDensity: (density: 2 | 3) => void
}

// Design §6.2: result count (number 600), a 32px sort select, and the view switcher in accent.
export function ControlBar({
  count,
  unit,
  sort,
  onSort,
  view,
  onView,
  uiLanguage,
  density,
  onDensity,
}: ControlBarProps) {
  const { t } = useTranslation()
  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
      <p className="mr-auto text-13 text-text-2" aria-live="polite">
        <b className="font-semibold text-text-1 tabular-nums">
          {new Intl.NumberFormat(uiLanguage).format(count)}
        </b>{" "}
        {t(`filters.countUnits.${unit}`)}
      </p>
      <label className="flex items-center gap-1.5 text-12 text-text-3">
        <span>{t("filters.sort")}</span>
        <select
          value={sort}
          onChange={(event) => {
            const next = QUERY_SORTS.find((item) => item === event.target.value)
            if (next) onSort(next)
          }}
          className="h-8 rounded-control border border-border bg-surface-1 px-2 text-13 text-text-1"
        >
          {QUERY_SORTS.map((item) => (
            <option key={item} value={item}>
              {t(`filters.sorts.${item}`)}
            </option>
          ))}
        </select>
      </label>
      {view === "grid" && (
        <Segmented
          label={t("filters.density")}
          size="sm"
          className="md:hidden"
          options={[
            { value: "2", label: "2" },
            { value: "3", label: "3" },
          ]}
          value={String(density) as "2" | "3"}
          onChange={(value) => {
            onDensity(value === "3" ? 3 : 2)
          }}
        />
      )}
      <Segmented
        label={t("filters.view")}
        tone="accent"
        size="sm"
        options={VIEW_MODES.map((mode) => ({ value: mode, label: t(`filters.views.${mode}`) }))}
        value={view}
        onChange={onView}
      />
    </div>
  )
}
