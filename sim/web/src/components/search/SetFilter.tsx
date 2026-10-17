import { useTranslation } from "react-i18next"

import type { SearchOptions } from "../../data"
import type { NameSource } from "../../domain/nameDisplay"
import type { TextLang } from "../../domain/search"
import { setCaption, setLabel } from "../../domain/sets"
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
    publicCode: set.publicCode ?? set.code,
    kind: set.kind,
    label: setCaption(set, textLang).text,
    fullWidth: true,
  }))
  const kindOrder = ["booster", "collaboration", "special_pack", "promo", "deck", "special"]
  const groups = kindOrder.flatMap((kind) => {
    const kindSets = options.filter((set) => set.kind === kind)
    if (kind !== "deck") return kindSets.length ? [kindSets] : []

    // This is display grouping only; card ownership always comes from home_set_id. The data has no deck-series field yet, so use that authored field if one is added.
    const deckGroups = new Map<string, typeof kindSets>()
    for (const set of kindSets) {
      const prefix = /^[A-Za-z]+/u.exec(set.publicCode)?.[0] ?? ""
      const group = deckGroups.get(prefix) ?? []
      group.push(set)
      deckGroups.set(prefix, group)
    }
    return [...deckGroups.values()]
  })
  const knownKinds = new Set(kindOrder)
  const unknownSets = options.filter((set) => !knownKinds.has(set.kind ?? ""))
  if (unknownSets.length) groups.push(unknownSets)
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
          label={setCaption(promo, textLang).text}
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
