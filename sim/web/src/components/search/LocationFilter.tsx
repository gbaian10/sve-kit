import { useTranslation } from "react-i18next"

import type { SearchOptions } from "../../data"
import { leaderFrameTokens, LOCATION_ORDER, tokenKindTokens } from "../../domain/deckLocation"
import type { TextLang } from "../../domain/search"
import { locationLabel, vocabularyOptionLabel } from "./facet-labels"
import { type FacetChip, FacetChips } from "./FacetChips"

// Where a printing goes in a deck; a chip's sub-values (Token kinds, leader frames) sit behind its
// popover like the former type filter's did, so their tokens and URLs are unchanged.
export function LocationFilter({
  options,
  textLang,
  selected,
  onChange,
}: {
  readonly options:
    | { readonly types: SearchOptions["types"]; readonly counts: SearchOptions["locationCounts"] }
    | undefined
  readonly textLang: TextLang
  readonly selected: readonly string[]
  readonly onChange: (selected: string[]) => void
}) {
  const { t } = useTranslation()
  const counts = options?.counts ?? {}
  const typeLabel = (code: string) => {
    const type = options?.types.find((item) => item.code === code)
    return type ? vocabularyOptionLabel(type.name, textLang) : code
  }
  const items = LOCATION_ORDER.flatMap((code): FacetChip[] => {
    // Rows the edition has no printings for stay out, as the type row does; "main" always shows.
    if (code !== "main" && !(counts[code] ?? 0) && options) return []
    const label = locationLabel(t, code)
    if (code === "token")
      return [
        {
          code,
          label,
          children: tokenKindTokens(counts).map((token) => ({
            code: token,
            label: typeLabel(token.slice("token:".length)),
          })),
        },
      ]
    if (code === "leader")
      return [
        {
          code,
          label,
          children: leaderFrameTokens(counts).map((token) => ({
            code: token,
            label: locationLabel(t, token),
          })),
        },
      ]
    return [{ code, label }]
  })
  return (
    <FacetChips
      title={t("locationFilter.label")}
      items={items}
      counts={options?.counts}
      selected={selected}
      onChange={onChange}
    />
  )
}
