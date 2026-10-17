import { useTranslation } from "react-i18next"

import { LIST_COLUMNS, prefsStore, usePrefs } from "../../settings"

export function ListColumnPicker() {
  const { t } = useTranslation()
  const { listVisibility } = usePrefs()
  return (
    <details className="relative text-13" data-list-column-picker="">
      <summary className="cursor-pointer rounded-control border border-border bg-surface-1 px-2 py-1.5">
        {t("results.visibleColumns")}
      </summary>
      <fieldset className="absolute left-0 z-10 mt-1 flex min-w-40 flex-col gap-2 rounded-control border border-border bg-surface-1 p-3 shadow-lg">
        <legend className="sr-only">{t("results.visibleColumns")}</legend>
        {LIST_COLUMNS.map((column) => (
          <label key={column} className="flex items-center gap-2 whitespace-nowrap">
            <input
              type="checkbox"
              checked={listVisibility[column]}
              disabled={
                (column === "number" && !listVisibility.name) ||
                (column === "name" && !listVisibility.number)
              }
              onChange={(event) => {
                prefsStore.set({
                  listVisibility: { ...listVisibility, [column]: event.target.checked },
                })
              }}
            />
            {column === "cost" || column === "attack" || column === "defense"
              ? t(`cardPage.stats.${column}`)
              : t(`results.columns.${column}`)}
          </label>
        ))}
      </fieldset>
    </details>
  )
}
