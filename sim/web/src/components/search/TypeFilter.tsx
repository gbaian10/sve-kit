import { useTranslation } from "react-i18next"

import type { SearchOptions } from "../../data"
import type { TextLang } from "../../domain/search"
import { TYPE_ORDER } from "../../domain/typeFilter"
import { CardTypeBadge } from "../card/CardTypeBadge"
import { vocabularyOptionLabel } from "./facet-labels"
import { type FacetChip, FacetChips } from "./FacetChips"

/** The printed type row: follower, spell, amulet. Deck locations have their own row below it. */
export function TypeFilter({
  options,
  textLang,
  selected,
  onChange,
}: {
  readonly options: Pick<SearchOptions, "types" | "typeCounts"> | undefined
  readonly textLang: TextLang
  readonly selected: readonly string[]
  readonly onChange: (selected: string[]) => void
}) {
  const { t } = useTranslation()
  const items = TYPE_ORDER.flatMap((code): FacetChip[] => {
    const type = options?.types.find((item) => item.code === code)
    return type
      ? [
          {
            code,
            label: vocabularyOptionLabel(type.name, textLang),
            content: <CardTypeBadge code={code} decorative />,
          },
        ]
      : []
  })
  return (
    <FacetChips
      title={t("typeFilter.label")}
      items={items}
      counts={options?.typeCounts}
      selected={selected}
      onChange={onChange}
    />
  )
}
