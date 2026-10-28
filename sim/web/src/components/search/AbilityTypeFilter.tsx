import { useTranslation } from "react-i18next"

import advancedStandIcon from "../../assets/official/texticon/icon_advancestand.png"
import fanfareIcon from "../../assets/official/texticon/icon_fanfare.png"
import lastWordIcon from "../../assets/official/texticon/icon_lastword.png"
import quickIcon from "../../assets/official/texticon/icon_quick.png"
import standIcon from "../../assets/official/texticon/icon_stand.png"
import { ABILITY_TYPES, type AbilityType } from "../../domain/abilityTypes"
import type { MatchMode } from "../../domain/query/model"
import { Toggle } from "./FilterControls"
import { FilterDropdown } from "./FilterDropdown"
import { MatchModeControl } from "./MatchModeControl"

const ABILITY_TYPE_ICONS: Readonly<Partial<Record<AbilityType, string>>> = {
  quick: quickIcon,
  fanfare: fanfareIcon,
  last_words: lastWordIcon,
  advanced_act: advancedStandIcon,
  act: standIcon,
}

export function AbilityTypeFilter({
  options,
  selected,
  matchMode,
  onChange,
  onMatchChange,
}: {
  readonly options:
    | {
        readonly coverage: "pending" | "unknown" | "partial" | "complete"
        readonly unknownCards?: number | undefined
      }
    | undefined
  readonly selected: readonly AbilityType[]
  readonly matchMode: MatchMode
  readonly onChange: (value: readonly AbilityType[]) => void
  readonly onMatchChange: (value: MatchMode) => void
}) {
  const { t } = useTranslation()
  const disabled = !options || options.coverage === "pending" || options.coverage === "unknown"
  return (
    <FilterDropdown
      label={t("filters.abilityTypes")}
      selectedCount={selected.length}
      summary={
        selected.length
          ? selected.map((code) => t(`filters.abilityTypeNames.${code}`)).join("、")
          : t("rarityBadge.all")
      }
      onClear={
        selected.length
          ? () => {
              onChange([])
            }
          : undefined
      }
    >
      <MatchModeControl
        label={t("filters.abilityMatch")}
        value={matchMode}
        disabled={disabled}
        onChange={onMatchChange}
      />
      <div className="flex flex-wrap gap-2">
        {ABILITY_TYPES.map((code) => (
          <Toggle
            key={code}
            pressed={selected.includes(code)}
            className="h-10 min-w-0 gap-0.5 px-0.5 text-11"
            disabled={disabled}
            onClick={() => {
              onChange(
                selected.includes(code)
                  ? selected.filter((item) => item !== code)
                  : [...selected, code].sort(),
              )
            }}
          >
            {ABILITY_TYPE_ICONS[code] && (
              <img
                src={ABILITY_TYPE_ICONS[code]}
                alt=""
                className="size-3.5 shrink-0 object-contain"
              />
            )}
            {t(`filters.abilityTypeNames.${code}`)}
          </Toggle>
        ))}
      </div>
      <p className="mt-3 text-12 text-text-3" role="status">
        {!options || options.coverage === "pending"
          ? t("filters.keywordLoading")
          : options.unknownCards
            ? t("filters.abilityUnknownCount", { count: options.unknownCards })
            : disabled
              ? t("filters.abilityUnknown")
              : t(matchMode === "all" ? "filters.abilityAll" : "filters.abilityAny")}
      </p>
    </FilterDropdown>
  )
}
