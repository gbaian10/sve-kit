import { useTranslation } from "react-i18next"

import type { SearchOptions } from "../../data"
import type { TextLang } from "../../domain/search"
import { leaderFrameTokens, tokenKindTokens, TYPE_ORDER } from "../../domain/typeFilter"
import { type FacetChip, FacetChips } from "./FacetChips"

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
  const items = TYPE_ORDER.flatMap((code): FacetChip[] => {
    const type = typeOf(code)
    if (!type) return []
    // The maintainer wants the filter to say "Token" whatever the vocabulary's translation is.
    if (code === "token")
      return [
        {
          code,
          label: t("typeFilter.token"),
          children: tokenKindTokens(options?.typeCounts ?? {}).flatMap((code) => {
            const option = typeOf(code.slice("token:".length))
            return option ? [{ code, label: label(option) }] : []
          }),
        },
      ]
    if (code === "leader")
      return [
        {
          code,
          label: label(type),
          children: leaderFrameTokens(options?.typeCounts ?? {}).map((code) => {
            const frame = code.slice("leader:".length)
            return {
              code,
              label:
                frame === "gold" || frame === "white" || frame === "unknown"
                  ? t(`typeFilter.${frame}`)
                  : frame,
            }
          }),
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
