import { useTranslation } from "react-i18next"

import type { NameSource } from "../../domain/nameDisplay"
import type { TextLang } from "../../domain/search"
import { setLabel } from "../../domain/sets"
import { FacetFilter } from "./FacetFilter"

interface SetOption {
  readonly code: string
  readonly publicCode?: string
  readonly name: NameSource
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
    label: `${set.publicCode ?? set.code} ${setLabel(set.name, textLang).text}`,
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
