import { useTranslation } from "react-i18next"

import { defaultSecondarySort, QUERY_SORTS, type QueryState } from "../../domain/query/model"

export function SortControls({
  query,
  onChange,
}: {
  readonly query: QueryState
  readonly onChange: (query: QueryState) => void
}) {
  const { t } = useTranslation()
  return (
    <div className="grid w-max gap-2">
      {([false, true] as const).map((secondary) => {
        const sort = secondary ? query.secondarySort : query.sort
        const descending = (secondary ? query.secondaryOrder : query.order) === "desc"
        const label = secondary ? t("results.thenBy") : t("results.sort")
        const direction =
          sort === "set"
            ? descending
              ? t("results.newestFirst")
              : t("results.oldestFirst")
            : descending
              ? t("results.largestFirst")
              : t("results.smallestFirst")
        return (
          <div key={String(secondary)} className="flex items-center gap-2 text-13">
            <select
              aria-label={label}
              value={sort ?? ""}
              className="h-8 w-auto shrink-0 rounded-control border border-border bg-surface-1 px-2 text-text-1"
              onChange={(event) => {
                const next = QUERY_SORTS.find((sort) => sort === event.target.value)
                if (secondary) {
                  const { secondaryOrder, ...rest } = query
                  onChange({
                    ...rest,
                    secondarySort: next ?? null,
                    ...(next && secondaryOrder === "desc" ? { secondaryOrder } : {}),
                  })
                } else if (next) {
                  if (query.secondarySort === next) {
                    const { secondaryOrder: _secondaryOrder, ...rest } = query
                    onChange({ ...rest, sort: next, secondarySort: defaultSecondarySort(next) })
                  } else onChange({ ...query, sort: next })
                }
              }}
            >
              {secondary && <option value="">{t("results.noSecondary")}</option>}
              {QUERY_SORTS.map((value) => (
                <option key={value} value={value} disabled={secondary && value === query.sort}>
                  {t(`results.sortBy.${value}`)}
                </option>
              ))}
            </select>
            {sort !== null ? (
              <button
                type="button"
                aria-label={`${label}：${direction}`}
                title={direction}
                aria-pressed={descending}
                className="h-8 shrink-0 rounded-control border border-border bg-surface-1 px-2 text-13 whitespace-nowrap"
                onClick={() => {
                  if (secondary) {
                    const { secondaryOrder, ...rest } = query
                    onChange(secondaryOrder === "desc" ? rest : { ...rest, secondaryOrder: "desc" })
                  } else {
                    const { order, ...rest } = query
                    onChange(order === "desc" ? rest : { ...rest, order: "desc" })
                  }
                }}
              >
                {direction} {descending ? "↓" : "↑"}
              </button>
            ) : (
              <button
                type="button"
                aria-label={label}
                disabled
                className="h-8 shrink-0 rounded-control border border-border bg-surface-1 px-2 text-13 whitespace-nowrap text-text-3 opacity-50"
              >
                {t("results.noSecondary")}
              </button>
            )}
          </div>
        )
      })}
    </div>
  )
}
