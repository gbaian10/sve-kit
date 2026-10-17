import { useTranslation } from "react-i18next"

import type { SearchOptions } from "../../data"
import type { NameSource } from "../../domain/nameDisplay"
import type { TextLang } from "../../domain/search"
import { setLabel } from "../../domain/sets"
import { FacetFilter } from "./FacetFilter"
import { Toggle } from "./FilterControls"

interface SetOption {
  readonly code: string
  readonly publicCode?: string
  readonly kind?: string
  readonly name: NameSource
}

export function SetFilter({
  sets,
  quickSets,
  counts,
  textLang,
  selected,
  onChange,
}: {
  readonly sets: readonly SetOption[] | undefined
  readonly quickSets: SearchOptions["quickSets"] | undefined
  readonly counts: Readonly<Record<string, number>> | undefined
  readonly textLang: TextLang
  readonly selected: readonly string[]
  readonly onChange: (selected: string[]) => void
}) {
  const { t } = useTranslation()
  const options = (sets ?? []).map((set) => ({
    code: set.code,
    kind: set.kind,
    label: `${set.publicCode ?? set.code} ${setLabel(set.name, textLang).text}`,
    fullWidth: true,
  }))
  const boosters = options.filter((set) => set.kind === "booster")
  const collaborations = options.filter((set) => set.kind === "collaboration")
  const otherSets = options.filter((set) => set.kind !== "booster" && set.kind !== "collaboration")
  const groups = [
    boosters,
    collaborations,
    [
      ...otherSets.filter((set) => set.kind === "promo"),
      ...otherSets.filter((set) => set.kind !== "promo"),
    ],
  ].filter((group) => group.length > 0)
  const toggleSet = (code: string) => {
    onChange(
      selected.includes(code)
        ? selected.filter((value) => value !== code)
        : [...selected, code].sort(),
    )
  }
  const recent = quickSets?.recent
  const promo = quickSets?.promo
  return (
    <div className="flex flex-wrap items-center gap-1.5">
      <FacetFilter
        label={t("filters.sets")}
        groups={groups}
        counts={counts}
        selected={selected}
        onChange={onChange}
      />
      {recent && (
        <Toggle
          pressed={selected.includes(recent.code)}
          disabled={!counts}
          label={t("quickSet.latestAccessible", { name: setLabel(recent.name, textLang).text })}
          title={setLabel(recent.name, textLang).text}
          onClick={() => {
            toggleSet(recent.code)
          }}
        >
          {t("quickSet.latest", { code: recent.publicCode ?? recent.code })}
        </Toggle>
      )}
      {promo && (
        <Toggle
          pressed={selected.includes(promo.code)}
          disabled={!counts}
          label={`${promo.publicCode ?? promo.code} ${setLabel(promo.name, textLang).text}`}
          title={setLabel(promo.name, textLang).text}
          onClick={() => {
            toggleSet(promo.code)
          }}
        >
          {promo.publicCode ?? promo.code}
        </Toggle>
      )}
    </div>
  )
}
