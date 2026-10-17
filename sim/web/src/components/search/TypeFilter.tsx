import { useTranslation } from "react-i18next"

import type { SearchOptions } from "../../data"
import type { TextLang } from "../../domain/search"
import { FacetFilter } from "./FacetFilter"

const TYPE_GROUPS = [["follower", "spell", "amulet", "token"], ["leader"], ["ep", "sep"]] as const
const TOKEN_TYPES = ["follower", "spell", "amulet", "crest", "equipment"] as const

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
  const label = (type: SearchOptions["types"][number]) =>
    type.name.translations[textLang] ?? type.name.original.text
  const groups = TYPE_GROUPS.map((codes) =>
    codes.flatMap((code) => {
      const type = options?.types.find((item) => item.code === code)
      if (!type) return []
      const children =
        code === "token"
          ? TOKEN_TYPES.flatMap((child) => {
              const option = options?.types.find((item) => item.code === child)
              return option && (options?.typeCounts[`token:${child}`] ?? 0) > 0
                ? [{ code: `token:${child}`, label: label(option) }]
                : []
            })
          : code === "leader"
            ? (["gold", "white", "unknown"] as const)
                .filter((frame) => (options?.typeCounts[`leader:${frame}`] ?? 0) > 0)
                .map((frame) => ({ code: `leader:${frame}`, label: t(`typeFilter.${frame}`) }))
            : []
      return [{ code, label: label(type), children, fullWidth: code === "leader" }]
    }),
  )
  return (
    <FacetFilter
      label={t("typeFilter.label")}
      groups={groups.filter((group) => group.length > 0)}
      counts={options?.typeCounts}
      selected={selected}
      onChange={onChange}
    />
  )
}
