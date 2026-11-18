import { GripVertical } from "lucide-react"
import { useCallback, useEffect, useId, useRef, useState } from "react"
import { useTranslation } from "react-i18next"

import { moveListColumn, prefsStore, usePrefs } from "../../settings"
import { useColumnReorder } from "./useColumnReorder"

export function ListColumnPicker() {
  const { t } = useTranslation()
  const { listVisibility, listOrder } = usePrefs()
  const reorder = useColumnReorder("vertical")
  const id = useId()
  const trigger = useRef<HTMLButtonElement>(null)
  const panel = useRef<HTMLFieldSetElement>(null)
  const [open, setOpen] = useState(false)
  const place = useCallback(() => {
    if (!trigger.current || !panel.current) return
    const { left, bottom } = trigger.current.getBoundingClientRect()
    panel.current.style.left = `${String(left)}px`
    panel.current.style.top = `${String(bottom)}px`
  }, [])
  useEffect(() => {
    if (!open) return
    window.addEventListener("resize", place)
    window.addEventListener("scroll", place, true)
    return () => {
      window.removeEventListener("resize", place)
      window.removeEventListener("scroll", place, true)
    }
  }, [open, place])

  return (
    <div className="relative text-13" data-list-column-picker="">
      <button
        ref={trigger}
        type="button"
        popoverTarget={id}
        aria-controls={id}
        aria-expanded={open}
        onClick={(event) => {
          event.preventDefault()
          const popup = panel.current
          if (!popup) return
          if (popup.matches(":popover-open")) {
            popup.hidePopover()
          } else {
            place()
            popup.showPopover({ source: event.currentTarget })
          }
        }}
        className="group block w-full cursor-pointer rounded-control border border-border bg-surface-1 px-2 py-1.5 text-left whitespace-nowrap"
      >
        <span className="list-item list-inside list-[disclosure-closed] group-aria-expanded:list-[disclosure-open]">
          {t("results.visibleColumns")}
        </span>
      </button>
      <fieldset
        ref={panel}
        id={id}
        popover="auto"
        onToggle={(event) => {
          setOpen(event.currentTarget.matches(":popover-open"))
        }}
        data-column-order=""
        className="fixed inset-auto m-0 mt-1 min-w-40 flex-col gap-2 overflow-visible rounded-control border border-border bg-surface-1 p-3 text-inherit shadow-lg open:flex"
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
    </div>
  )
}
