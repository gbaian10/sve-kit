import { useTranslation } from "react-i18next"

import type { SearchOptions } from "../../data"
import type { TextLang } from "../../domain/search"
import { TYPE_ORDER } from "../../domain/typeFilter"
import { CardTypeBadge } from "../card/CardTypeBadge"
import { vocabularyOptionLabel } from "./facet-labels"
import { type FacetChip, FacetChips } from "./FacetChips"
import { Section } from "./FilterControls"

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
  if (!options)
    return (
      <Section title={t("typeFilter.label")}>
        <div aria-hidden="true" className="flex gap-1.5">
          {[0, 1, 2].map((index) => (
            <span
              key={index}
              className="h-9 w-20 rounded-button border border-border bg-surface-2 motion-safe:animate-pulse"
            />
          ))}
        </div>
      </Section>
    )
  const items = TYPE_ORDER.flatMap((code): FacetChip[] => {
    const type = options.types.find((item) => item.code === code)
    if (!type) return []
    const label = vocabularyOptionLabel(type.name, textLang)
    return [{ code, label, content: <CardTypeBadge code={code} label={label} decorative /> }]
  })
  return (
    <FacetChips
      title={t("typeFilter.label")}
      items={items}
      counts={options.typeCounts}
      selected={selected}
      onChange={onChange}
    />
  )
}
