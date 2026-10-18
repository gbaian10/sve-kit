import { useTranslation } from "react-i18next"

import type { SearchOptions } from "../../data"
import type { MechanicState } from "../../domain/query/model"
import type { Region, TextLang } from "../../domain/search"
import { vocabularyOptionLabel } from "./facet-labels"
import { Section, Toggle } from "./FilterControls"

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
  const disabled = !options || options.coverage === "unknown"
  return (
    <Section
      title={t("filters.keywords")}
      onClear={
        Object.keys(selected).length
          ? () => {
              onChange({})
            }
          : undefined
      }
    >
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
      <p className="w-full text-12 text-text-3" role="status">
        {!options
          ? t("filters.keywordLoading")
          : disabled
            ? edition === "en"
              ? t("filters.keywordEnglishUnknown")
              : t("filters.keywordUnknown")
            : options.coverage === "partial"
              ? t("filters.keywordPartial")
              : t("filters.keywordAny")}
      </p>
    </Section>
  )
}
