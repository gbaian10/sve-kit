import { useTranslation } from "react-i18next"

import { QUERY_SORTS, type QueryState } from "../../domain/query/model"

export function SortControls({
  query,
  onChange,
}: {
  readonly query: QueryState
  readonly onChange: (query: QueryState) => void
}) {
  const { t } = useTranslation()
  return (
    <div className="grid w-full gap-1 md:w-auto">
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
          <div
            key={String(secondary)}
            className="grid grid-cols-[3rem_minmax(0,1fr)_minmax(0,1fr)] items-center gap-1 text-13 md:flex"
          >
            <label className="contents md:flex md:items-center md:gap-1">
              <span className="w-12 shrink-0">{label}</span>
              <select
                aria-label={label}
                value={sort ?? ""}
                className="h-7 w-full min-w-0 rounded-control border border-border bg-surface-1 px-1 text-text-1 md:h-8 md:w-24 md:px-2"
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
                      const nextQuery = { ...query, sort: next, secondarySort: null }
                      delete nextQuery.secondaryOrder
                      onChange(nextQuery)
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
            </label>
            {sort !== null ? (
              <button
                type="button"
                aria-label={
                  secondary ? `${label}：${t("results.descending")}` : t("results.descending")
                }
                aria-pressed={descending}
                className="h-7 w-full min-w-0 rounded-control border border-border bg-surface-1 px-1 text-13 md:h-8 md:min-w-24 md:px-2"
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
                className="h-7 w-full min-w-0 rounded-control border border-border bg-surface-1 px-1 text-13 text-text-3 opacity-50 md:h-8 md:min-w-24 md:px-2"
              />
            )}
          </div>
        )
      })}
    </div>
  )
}
