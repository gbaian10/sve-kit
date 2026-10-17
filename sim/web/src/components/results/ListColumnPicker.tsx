import { GripVertical } from "lucide-react"
import { useTranslation } from "react-i18next"

import { moveListColumn, prefsStore, usePrefs } from "../../settings"
import { useColumnReorder } from "./useColumnReorder"

export function ListColumnPicker() {
  const { t } = useTranslation()
  const { listVisibility, listOrder } = usePrefs()
  const reorder = useColumnReorder("vertical")
  return (
    <details className="relative text-13" data-list-column-picker="">
      <summary className="cursor-pointer rounded-control border border-border bg-surface-1 px-2 py-1.5">
        {t("results.visibleColumns")}
      </summary>
      <fieldset
        data-column-order=""
        className="absolute left-0 z-10 mt-1 flex min-w-40 flex-col gap-2 rounded-control border border-border bg-surface-1 p-3 shadow-lg"
      >
        <legend className="sr-only">{t("results.visibleColumns")}</legend>
        {listOrder.map((column, index) => {
          const label =
            column === "cost" || column === "attack" || column === "defense"
              ? t(`cardPage.stats.${column}`)
              : t(`results.columns.${column}`)
          return (
            <div
              key={column}
              data-order-column={column}
              data-drop={
                reorder.insertion?.column === column
                  ? reorder.insertion.after
                    ? "after"
                    : "before"
                  : undefined
              }
              className="relative flex items-center gap-2 whitespace-nowrap"
            >
              <button
                type="button"
                {...reorder.bindings(column)}
                data-column-reorder=""
                aria-label={t("results.reorderColumn", { column: label })}
                aria-keyshortcuts="ArrowUp ArrowDown"
                className="cursor-grab touch-none rounded-control px-1 text-text-3 focus-visible:outline-2"
                onKeyDown={(event) => {
                  if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return
                  event.preventDefault()
                  const after = event.key === "ArrowDown"
                  const target = listOrder[index + (after ? 1 : -1)]
                  if (target)
                    prefsStore.set({ listOrder: moveListColumn(listOrder, column, target, after) })
                }}
              >
                <GripVertical aria-hidden="true" className="size-4" />
              </button>
              <label className="flex items-center gap-2">
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
                {label}
              </label>
            </div>
          )
        })}
      </fieldset>
    </details>
  )
}
