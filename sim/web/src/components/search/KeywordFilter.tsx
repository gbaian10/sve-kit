import { useTranslation } from "react-i18next"

import assailIcon from "../../assets/keywords/assail.svg"
import auraIcon from "../../assets/keywords/aura.svg"
import baneIcon from "../../assets/keywords/bane.svg"
import drainIcon from "../../assets/keywords/drain.svg"
import guardIcon from "../../assets/keywords/guard.svg"
import intimidateIcon from "../../assets/keywords/intimidate.svg"
import type { SearchOptions } from "../../data"
import type { MechanicState } from "../../domain/query/model"
import type { Region, TextLang } from "../../domain/search"
import { vocabularyOptionLabel } from "./facet-labels"
import { Toggle } from "./FilterControls"
import { FilterDropdown } from "./FilterDropdown"

const KEYWORD_ORDER = [
  "kw:assail",
  "kw:intimidate",
  "kw:aura",
  "kw:ward",
  "kw:bane",
  "kw:drain",
  "kw:rush",
  "kw:storm",
] as const

const KEYWORD_ICONS: Readonly<Record<string, string | undefined>> = {
  "kw:assail": assailIcon,
  "kw:intimidate": intimidateIcon,
  "kw:aura": auraIcon,
  "kw:ward": guardIcon,
  "kw:bane": baneIcon,
  "kw:drain": drainIcon,
}

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
        readonly unknownCards?: number | undefined
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
      <div className="grid grid-cols-4 gap-2">
        {KEYWORD_ORDER.flatMap((id) => {
          const keyword = options?.keywords.find((item) => item.id === id)
          if (!keyword) return []
          const icon = KEYWORD_ICONS[id]
          return (
            <Toggle
              key={keyword.id}
              pressed={selected[keyword.id] === "has"}
              className="h-10 min-w-0 gap-0.5 px-0.5 text-11"
              disabled={disabled}
              onClick={() => {
                onChange(
                  selected[keyword.id] === "has"
                    ? Object.fromEntries(
                        Object.entries(selected).filter(([id]) => id !== keyword.id),
                      )
                    : { ...selected, [keyword.id]: "has" },
                )
              }}
            >
              {icon && <img src={icon} alt="" className="size-3.5 shrink-0 object-contain" />}
              {vocabularyOptionLabel(keyword.name, textLang)}
            </Toggle>
          )
        })}
      </div>
      <p className="mt-3 text-12 text-text-3" role="status">
        {!options || options.coverage === "pending"
          ? t("filters.keywordLoading")
          : options.unknownCards !== undefined && options.unknownCards > 0
            ? t("filters.keywordUnknownCount", { count: options.unknownCards })
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
