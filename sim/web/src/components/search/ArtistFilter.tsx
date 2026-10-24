import { useTranslation } from "react-i18next"

import type { SearchOptions } from "../../data"
import type { TextLang } from "../../domain/search"
import { FacetFilter } from "./FacetFilter"

// Artists are credited as the official site prints them, so there is nothing to translate: the
// list is the printed names, ordered in the UI language's collation, with a search box.
export function ArtistFilter({
  options,
  textLang,
  selected,
  onChange,
}: {
  readonly options:
    | { readonly artists: SearchOptions["artists"]; readonly counts: SearchOptions["artistCounts"] }
    | undefined
  readonly textLang: TextLang
  readonly selected: readonly string[]
  readonly onChange: (selected: string[]) => void
}) {
  const { t } = useTranslation()
  const collator = new Intl.Collator(textLang === "zh-Hant" ? "zh-Hant" : textLang)
  const items = (options?.artists ?? [])
    .map((artist) => ({ code: artist.id, label: artist.name }))
    .sort((a, b) => collator.compare(a.label, b.label) || a.code.localeCompare(b.code))
  return (
    <FacetFilter
      label={t("filters.artists")}
      groups={[items]}
      counts={options?.counts}
      selected={selected}
      onChange={onChange}
      searchable
    />
  )
}
