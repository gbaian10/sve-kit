import { useTranslation } from "react-i18next"

import {
  CLASS_MECHANISMS,
  COLLAB_MECHANISMS,
  type MechanismOptions,
  type PrintedMechanism,
} from "../../domain/printedMechanisms"
import type { MatchMode } from "../../domain/query/model"
import { Toggle } from "./FilterControls"
import { FilterDropdown } from "./FilterDropdown"
import { MatchModeControl } from "./MatchModeControl"

export function PrintedMechanismFilter({
  group,
  options,
  selected,
  matchMode,
  onChange,
  onMatchChange,
}: {
  readonly group: "classMechanisms" | "collabMechanisms"
  readonly options: MechanismOptions | undefined
  readonly selected: readonly PrintedMechanism[]
  readonly matchMode: MatchMode
  readonly onChange: (value: readonly PrintedMechanism[]) => void
  readonly onMatchChange: (value: MatchMode) => void
}) {
  const { t } = useTranslation()
  const disabled = !options || options.coverage === "pending" || options.coverage === "unknown"
  const codes = group === "classMechanisms" ? CLASS_MECHANISMS : COLLAB_MECHANISMS
  return (
    <FilterDropdown
      label={t(`filters.${group}`)}
      selectedCount={selected.length}
      summary={
        selected.length
          ? selected.map((code) => t(`filters.printedMechanismNames.${code}`)).join("、")
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
        label={t("filters.mechanismMatch")}
        value={matchMode}
        disabled={disabled}
        onChange={onMatchChange}
      />
      <div className="flex flex-wrap gap-2">
        {codes.map((code) => (
          <Toggle
            key={code}
            pressed={selected.includes(code)}
            disabled={disabled}
            onClick={() => {
              onChange(
                selected.includes(code)
                  ? selected.filter((item) => item !== code)
                  : [...selected, code].sort(),
              )
            }}
          >
            {t(`filters.printedMechanismNames.${code}`)}
            {options?.counts[code] !== undefined && (
              <span className="ml-1 text-11 text-text-3">{options.counts[code]}</span>
            )}
          </Toggle>
        ))}
      </div>
      <p className="mt-3 text-12 text-text-3" role="status">
        {!options || options.coverage === "pending"
          ? t("filters.keywordLoading")
          : options.unknownCards
            ? t("filters.mechanismUnknownCount", { count: options.unknownCards })
            : disabled
              ? t("filters.mechanismUnknown")
              : t("filters.mechanismPrinted")}
      </p>
    </FilterDropdown>
  )
}
