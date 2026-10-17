import { useTranslation } from "react-i18next"

import type { TextLang } from "../../domain/search"
import { FacetFilter } from "./FacetFilter"

interface SetOption {
  readonly code: string
  readonly publicCode?: string
  readonly name: {
    readonly original: { readonly text: string }
    readonly translations: Partial<Record<TextLang, string>>
  }
}

export function SetFilter({
  sets,
  counts,
  textLang,
  selected,
  onChange,
}: {
  readonly sets: readonly SetOption[] | undefined
  readonly counts: Readonly<Record<string, number>> | undefined
  readonly textLang: TextLang
  readonly selected: readonly string[]
  readonly onChange: (selected: string[]) => void
}) {
  const { t } = useTranslation()
  const options = (sets ?? []).map((set) => ({
    code: set.code,
    label: `${set.publicCode ?? set.code} ${set.name.translations[textLang] ?? set.name.original.text}`,
    fullWidth: true,
  }))
  return (
    <FacetFilter
      label={t("filters.sets")}
      groups={options.length > 0 ? [options] : []}
      counts={counts}
      selected={selected}
      onChange={onChange}
    />
  )
}
