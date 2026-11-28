import { Fragment, useId } from "react"
import { useTranslation } from "react-i18next"

import { classStyle } from "../../domain/classes"
import {
  CLASS_MECHANISM_GROUPS,
  COLLAB_MECHANISM_GROUPS,
  isAbbreviatedMechanism,
  type MechanismOptions,
  type PrintedMechanism,
} from "../../domain/printedMechanisms"
import type { MatchMode } from "../../domain/query/model"
import { CLASS_ICON } from "../card/classIcons"
import { Toggle } from "./FilterControls"
import { FilterDropdown } from "./FilterDropdown"
import { MatchModeControl } from "./MatchModeControl"

export function PrintedMechanismFilter({
  group,
  classLabel,
  options,
  selected,
  matchMode,
  onChange,
  onMatchChange,
}: {
  readonly group: "classMechanisms" | "collabMechanisms"
  /** The snapshot's name for a class code. */
  readonly classLabel: (code: string) => string
  readonly options: MechanismOptions | undefined
  readonly selected: readonly PrintedMechanism[]
  readonly matchMode: MatchMode
  readonly onChange: (value: readonly PrintedMechanism[]) => void
  readonly onMatchChange: (value: MatchMode) => void
}) {
  const { t } = useTranslation()
  const id = useId()
  const disabled = !options || options.coverage === "pending" || options.coverage === "unknown"
  const rows =
    group === "classMechanisms"
      ? CLASS_MECHANISM_GROUPS.map(({ classCode, mechanisms }) => {
          const name = classLabel(classCode)
          const labelId = `${id}-${classCode}`
          return {
            key: classCode,
            mechanisms,
            groupLabel: { "aria-labelledby": labelId },
            heading: (
              <img
                id={labelId}
                src={CLASS_ICON[classStyle(classCode)]}
                alt={name}
                title={name}
                className="size-6"
              />
            ),
          }
        })
      : COLLAB_MECHANISM_GROUPS.map(({ titleCode, mechanisms }) => {
          const name = t(`filters.collabTitleNames.${titleCode}`)
          return {
            key: titleCode,
            mechanisms,
            groupLabel: { "aria-label": name },
            heading: (
              <abbr title={name} className="text-12 text-text-3 no-underline">
                {t(`filters.collabTitleShortNames.${titleCode}`)}
              </abbr>
            ),
          }
        })
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
      {/* One row per class or title, its name in a shared first column. A row that cannot fit
          scrolls sideways on its own; the padding keeps the focus outline inside the scroller. */}
      <div className="grid grid-cols-[auto_minmax(0,1fr)] items-center gap-x-1.5 gap-y-1">
        {rows.map((row) => (
          <Fragment key={row.key}>
            {row.heading}
            <div
              role="group"
              {...row.groupLabel}
              className="-m-1 flex gap-1.5 overflow-x-auto overscroll-x-contain p-1"
            >
              {row.mechanisms.map((code: PrintedMechanism) => (
                <Toggle
                  key={code}
                  pressed={selected.includes(code)}
                  disabled={disabled}
                  title={
                    isAbbreviatedMechanism(code)
                      ? t(`filters.printedMechanismAbbreviations.${code}`)
                      : undefined
                  }
                  className="shrink-0 px-2! text-12!"
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
          </Fragment>
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
