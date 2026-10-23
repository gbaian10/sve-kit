import { useTranslation } from "react-i18next"

import type { SearchOptions } from "../../data"
import type { MechanicState } from "../../domain/query/model"
import type { Region, TextLang } from "../../domain/search"
import { vocabularyOptionLabel } from "./facet-labels"
import { Toggle } from "./FilterControls"
import { FilterDropdown } from "./FilterDropdown"

export function KeywordFilter({
  options,
  textLang,
  edition,
  selected,
  onChange,
}: {
  readonly options:
    | {
        readonly keywords: SearchOptions["keywords"]
        readonly coverage: SearchOptions["keywordCoverage"]
      }
    | undefined
  readonly textLang: TextLang
  readonly edition: Region
  readonly selected: Readonly<Record<string, MechanicState>>
  readonly onChange: (selected: Readonly<Record<string, MechanicState>>) => void
}) {
  const { t } = useTranslation()
  const disabled = !options || options.coverage === "pending" || options.coverage === "unknown"
  return (
    <FilterDropdown
      label={t("filters.keywords")}
      selectedCount={Object.keys(selected).length}
      summary={
        Object.keys(selected).length > 0
          ? Object.keys(selected)
              .map((id) => {
                const keyword = options?.keywords.find((item) => item.id === id)
                const label = keyword ? vocabularyOptionLabel(keyword.name, textLang) : id
                return selected[id] === "not" ? t("filters.keywordNot", { label }) : label
              })
              .join("、")
          : !options || options.coverage === "pending"
            ? t("filters.keywordLoading")
            : disabled
              ? t(edition === "en" ? "filters.keywordEnglishUnknown" : "filters.keywordUnknown")
              : t("rarityBadge.all")
      }
      onClear={
        Object.keys(selected).length
          ? () => {
              onChange({})
            }
          : undefined
      }
    >
      <div className="flex flex-wrap gap-2">
        {(options?.keywords ?? []).map((keyword) => (
          <Toggle
            key={keyword.id}
            pressed={selected[keyword.id] === "has"}
            disabled={disabled}
            onClick={() => {
              onChange(
                selected[keyword.id] === "has"
                  ? Object.fromEntries(Object.entries(selected).filter(([id]) => id !== keyword.id))
                  : { ...selected, [keyword.id]: "has" },
              )
            }}
          >
            {vocabularyOptionLabel(keyword.name, textLang)}
          </Toggle>
        ))}
      </div>
      <p className="mt-3 text-12 text-text-3" role="status">
        {!options || options.coverage === "pending"
          ? t("filters.keywordLoading")
          : disabled
            ? edition === "en"
              ? t("filters.keywordEnglishUnknown")
              : t("filters.keywordUnknown")
            : options.coverage === "partial"
              ? t("filters.keywordPartial")
              : t("filters.keywordAny")}
      </p>
    </FilterDropdown>
  )
}
