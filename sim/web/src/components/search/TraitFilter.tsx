import { useTranslation } from "react-i18next"

import type { SearchOptions } from "../../data"
import type { TextLang } from "../../domain/search"
import { vocabularyOptionLabel } from "./facet-labels"
import { FacetFilter } from "./FacetFilter"

// Traits are many (over a hundred), so they live in the sets' kind of dropdown with a search box,
// ordered by their label in the UI's text language.
export function TraitFilter({
  options,
  textLang,
  selected,
  onChange,
}: {
  readonly options:
    | { readonly traits: SearchOptions["traits"]; readonly counts: SearchOptions["traitCounts"] }
    | undefined
  readonly textLang: TextLang
  readonly selected: readonly string[]
  readonly onChange: (selected: string[]) => void
}) {
  const { t } = useTranslation()
  const collator = new Intl.Collator(textLang === "zh-Hant" ? "zh-Hant" : textLang)
  const items = (options?.traits ?? [])
    .map((trait) => ({ code: trait.code, label: vocabularyOptionLabel(trait.name, textLang) }))
    .sort((a, b) => collator.compare(a.label, b.label) || a.code.localeCompare(b.code))
  return (
    <FacetFilter
      label={t("filters.traits")}
      groups={[items]}
      counts={options?.counts}
      selected={selected}
      onChange={onChange}
      searchable
    />
  )
}
