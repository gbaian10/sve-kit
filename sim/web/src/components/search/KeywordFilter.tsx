import { useId, useState } from "react"
import { useTranslation } from "react-i18next"

import assailIcon from "../../assets/keywords/assail.svg"
import auraIcon from "../../assets/keywords/aura.svg"
import baneIcon from "../../assets/keywords/bane.svg"
import drainIcon from "../../assets/keywords/drain.svg"
import guardIcon from "../../assets/keywords/guard.svg"
import intimidateIcon from "../../assets/keywords/intimidate.svg"
import type { SearchOptions } from "../../data"
import {
  MECHANIC_RELATIONS,
  mechanicKey,
  type MechanicRelation,
  PANEL_KEYWORDS,
  parseMechanicKey,
} from "../../domain/mechanics"
import type { MatchMode } from "../../domain/query/model"
import type { Region, TextLang } from "../../domain/search"
import { cn } from "../ui/cn"
import { rovingRadioKeyDown } from "../ui/useRovingRadio"
import { vocabularyOptionLabel } from "./facet-labels"
import { Toggle } from "./FilterControls"
import { FilterDropdown } from "./FilterDropdown"
import { MatchModeControl } from "./MatchModeControl"
import { mechanicConditionLabel } from "./mechanic-labels"

const KEYWORD_ICONS: Readonly<Record<string, string | undefined>> = {
  "kw:assail": assailIcon,
  "kw:intimidate": intimidateIcon,
  "kw:aura": auraIcon,
  "kw:ward": guardIcon,
  "kw:bane": baneIcon,
  "kw:drain": drainIcon,
}

export function KeywordFilter({
  options,
  textLang,
  edition,
  selected,
  onChange,
  matchMode = "any",
  onMatchChange,
}: {
  readonly options:
    | {
        readonly keywords: SearchOptions["keywords"]
        readonly unknownCards?: SearchOptions["keywordUnknownCards"]
        readonly coverage: SearchOptions["keywordCoverage"]
      }
    | undefined
  readonly matchMode?: MatchMode
  readonly onMatchChange?: (value: MatchMode) => void
  readonly textLang: TextLang
  readonly edition: Region
  /** `mechanicKey` of each wanted keyword. */
  readonly selected: readonly string[]
  readonly onChange: (selected: readonly string[]) => void
}) {
  const { t } = useTranslation()
  const id = useId()
  const [relation, setRelation] = useState<MechanicRelation>("has")
  const coverage = options?.coverage[relation]
  const loading = !options || coverage === "pending"
  const disabled = loading || coverage === "unknown"
  const unknownCards = options?.unknownCards?.[relation]
  const keywordLabel = (keywordId: string) => {
    const keyword = options?.keywords.find((item) => item.id === keywordId)
    return keyword ? vocabularyOptionLabel(keyword.name, textLang) : keywordId
  }
  const countOf = (target: MechanicRelation) =>
    selected.filter((key) => parseMechanicKey(key)?.relation === target).length
  return (
    <FilterDropdown
      label={t("filters.keywords")}
      selectedCount={selected.length}
      summary={
        selected.length > 0
          ? selected
              .map((key) => {
                const condition = parseMechanicKey(key)
                // The dropdown title already says "keyword", so a held keyword shows its bare name.
                return condition?.relation === "has"
                  ? keywordLabel(condition.keywordId)
                  : mechanicConditionLabel(t, key, keywordLabel)
              })
              .join("、")
          : !options || options.coverage.has === "pending"
            ? t("filters.keywordLoading")
            : options.coverage.has === "unknown"
              ? t(edition === "en" ? "filters.keywordEnglishUnknown" : "filters.keywordUnknown")
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
      {onMatchChange && (
        <MatchModeControl
          label={t("filters.keywordMatch")}
          value={matchMode}
          disabled={!options}
          onChange={onMatchChange}
        />
      )}
      <div
        role="tablist"
        aria-label={t("filters.keywordRelation")}
        className="grid grid-cols-2 border-b border-border"
      >
        {MECHANIC_RELATIONS.map((value) => {
          const active = value === relation
          const count = countOf(value)
          return (
            <button
              key={value}
              type="button"
              role="tab"
              id={`${id}-${value}`}
              aria-selected={active}
              aria-controls={`${id}-panel`}
              tabIndex={active ? 0 : -1}
              onClick={() => {
                setRelation(value)
              }}
              onKeyDown={(event) => {
                rovingRadioKeyDown(event, MECHANIC_RELATIONS, relation, setRelation, id)
              }}
              className={cn(
                "-mb-px flex h-9 min-w-0 items-center justify-center gap-1.5 border-b-2 px-2 text-13 font-semibold",
                "focus-visible:outline-2 focus-visible:outline-offset-2",
                active
                  ? "border-text-1 text-text-1"
                  : "border-transparent text-text-2 hover:text-text-1",
              )}
            >
              <span className="truncate">{t(`filters.keywordRelations.${value}`)}</span>
              {count > 0 && (
                <span className="min-w-4.5 shrink-0 rounded-pill bg-text-1 px-1 text-11 leading-4.5 text-surface-1 tabular-nums">
                  <span aria-hidden="true">{count}</span>
                  <span className="sr-only">{t("filters.keywordRelationSelected", { count })}</span>
                </span>
              )}
            </button>
          )
        })}
      </div>
      <div role="tabpanel" id={`${id}-panel`} aria-labelledby={`${id}-${relation}`}>
        <p className="mt-2 text-12 text-text-2">{t(`filters.keywordRelationHints.${relation}`)}</p>
        {/* Gaps in the tags sit above the buttons so a short popover never hides them. */}
        <p className="mt-1 text-12 text-text-2 empty:hidden" role="status">
          {loading
            ? t("filters.keywordLoading")
            : disabled
              ? edition === "en"
                ? t("filters.keywordEnglishUnknown")
                : t("filters.keywordUnknown")
              : unknownCards !== undefined && unknownCards > 0
                ? t("filters.keywordUnknownCount", {
                    count: unknownCards,
                    relation: t(`filters.keywordRelations.${relation}`),
                  })
                : null}
        </p>
        <div className="mt-2 grid grid-cols-4 gap-2">
          {PANEL_KEYWORDS.flatMap((keywordId) => {
            const keyword = options?.keywords.find((item) => item.id === keywordId)
            if (!keyword) return []
            const key = mechanicKey(relation, keywordId)
            const pressed = selected.includes(key)
            const icon = KEYWORD_ICONS[keywordId]
            return (
              <Toggle
                key={keywordId}
                pressed={pressed}
                className="h-10 min-w-0 gap-0.5 px-0.5 text-11"
                disabled={disabled}
                onClick={() => {
                  onChange(pressed ? selected.filter((other) => other !== key) : [...selected, key])
                }}
              >
                {icon && <img src={icon} alt="" className="size-3.5 shrink-0 object-contain" />}
                {vocabularyOptionLabel(keyword.name, textLang)}
              </Toggle>
            )
          })}
        </div>
      </div>
    </FilterDropdown>
  )
}
