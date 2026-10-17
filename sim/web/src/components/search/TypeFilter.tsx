import { useTranslation } from "react-i18next"

import type { SearchOptions } from "../../data"
import type { TextLang } from "../../domain/search"
import { type FacetChip, FacetChips } from "./FacetChips"

const TYPE_ORDER = ["follower", "spell", "amulet", "token", "leader", "ep", "sep"] as const
const TOKEN_TYPES = ["follower", "spell", "amulet", "crest", "equipment"] as const
const LEADER_FRAMES = ["gold", "white", "unknown"] as const

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
  const typeOf = (code: string) => options?.types.find((item) => item.code === code)
  const label = (type: SearchOptions["types"][number]) =>
    type.name.translations[textLang] ?? type.name.original.text
  const count = (code: string) => options?.typeCounts[code] ?? 0
  const items = TYPE_ORDER.flatMap((code): FacetChip[] => {
    const type = typeOf(code)
    if (!type) return []
    // The maintainer wants the filter to say "Token" whatever the vocabulary's translation is.
    if (code === "token")
      return [
        {
          code,
          label: t("typeFilter.token"),
          children: TOKEN_TYPES.flatMap((child) => {
            const option = typeOf(child)
            return option && count(`token:${child}`) > 0
              ? [{ code: `token:${child}`, label: label(option) }]
              : []
          }),
        },
      ]
    if (code === "leader")
      return [
        {
          code,
          label: label(type),
          children: LEADER_FRAMES.filter((frame) => count(`leader:${frame}`) > 0).map((frame) => ({
            code: `leader:${frame}`,
            label: t(`typeFilter.${frame}`),
          })),
        },
      ]
    return [{ code, label: label(type) }]
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
