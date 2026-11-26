import { X } from "lucide-react"
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
import type { MatchMode, MechanicState } from "../../domain/query/model"
import type { Region, TextLang } from "../../domain/search"
import { cn } from "../ui/cn"
import { rovingRadioKeyDown } from "../ui/useRovingRadio"
import { vocabularyOptionLabel } from "./facet-labels"
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

type ChoiceState = MechanicState | "off"
// One click wants the keyword, the next excludes it, the third clears the condition.
const NEXT_STATE: Readonly<Record<ChoiceState, ChoiceState>> = {
  off: "has",
  has: "not",
  not: "off",
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
  readonly selected: Readonly<Record<string, MechanicState>>
  readonly onChange: (selected: Readonly<Record<string, MechanicState>>) => void
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
  const keys = Object.keys(selected)
  const countOf = (target: MechanicRelation) =>
    keys.filter((key) => parseMechanicKey(key)?.relation === target).length
  return (
    <FilterDropdown
      label={t("filters.keywords")}
      selectedCount={keys.length}
      summary={
        keys.length > 0
          ? keys
              .map((key) => {
                const state = selected[key] ?? "has"
                const condition = parseMechanicKey(key)
                // The dropdown title already says "keyword", so a held keyword shows its bare name.
                return condition?.relation === "has" && state === "has"
                  ? keywordLabel(condition.keywordId)
                  : mechanicConditionLabel(t, key, state, keywordLabel)
              })
              .join("、")
          : !options || options.coverage.has === "pending"
            ? t("filters.keywordLoading")
            : options.coverage.has === "unknown"
              ? t(edition === "en" ? "filters.keywordEnglishUnknown" : "filters.keywordUnknown")
              : t("rarityBadge.all")
      }
      onClear={
        keys.length
          ? () => {
              onChange({})
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
      {/* Four equal columns keep every relation on one row at any panel width. */}
      <div
        role="tablist"
        aria-label={t("filters.keywordRelation")}
        className="grid grid-cols-4 border-b border-border"
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
              tabIndex={active ? 0 : -1}
              onClick={() => {
                setRelation(value)
              }}
              onKeyDown={(event) => {
                rovingRadioKeyDown(event, MECHANIC_RELATIONS, relation, setRelation, id)
              }}
              className={cn(
                "relative -mb-px h-9 min-w-0 border-b-2 px-1 text-12 font-semibold",
                "focus-visible:outline-2 focus-visible:outline-offset-2",
                active
                  ? "border-text-1 text-text-1"
                  : "border-transparent text-text-2 hover:text-text-1",
              )}
            >
              <span className="block truncate">{t(`filters.keywordRelations.${value}`)}</span>
              {count > 0 && (
                <span className="absolute top-0 right-0 min-w-4 rounded-pill bg-text-1 px-1 text-11 leading-4 text-surface-1 tabular-nums">
                  {count}
                </span>
              )}
            </button>
          )
        })}
      </div>
      <div role="tabpanel" aria-labelledby={`${id}-${relation}`}>
        <p className="mt-2 text-12 text-text-2">{t(`filters.keywordRelationHints.${relation}`)}</p>
        <div className="mt-2 grid grid-cols-4 gap-2">
          {PANEL_KEYWORDS.flatMap((keywordId) => {
            const keyword = options?.keywords.find((item) => item.id === keywordId)
            if (!keyword) return []
            const key = mechanicKey(relation, keywordId)
            const state: ChoiceState = selected[key] ?? "off"
            const label = vocabularyOptionLabel(keyword.name, textLang)
            const icon = KEYWORD_ICONS[keywordId]
            return (
              <button
                key={keywordId}
                type="button"
                aria-pressed={state !== "off"}
                aria-label={
                  state === "off"
                    ? undefined
                    : t(state === "has" ? "filters.keywordWanted" : "filters.keywordExcluded", {
                        label,
                      })
                }
                disabled={disabled}
                onClick={() => {
                  const next = NEXT_STATE[state]
                  const rest = Object.fromEntries(
                    Object.entries(selected).filter(([other]) => other !== key),
                  )
                  onChange(next === "off" ? rest : { ...rest, [key]: next })
                }}
                className={cn(
                  "flex h-10 min-w-0 items-center justify-center gap-0.5 rounded-control border px-0.5 text-11 font-semibold whitespace-nowrap",
                  "disabled:cursor-not-allowed disabled:opacity-50",
                  state === "has" &&
                    "border-text-1 bg-surface-3 text-text-1 ring-1 ring-text-1 ring-inset",
                  state === "not" &&
                    "border-danger bg-danger-soft text-danger line-through ring-1 ring-danger ring-inset",
                  state === "off" && "border-border bg-surface-1 text-text-2 hover:bg-surface-2",
                )}
              >
                {state === "not" ? (
                  <X aria-hidden="true" className="size-3.5 shrink-0" />
                ) : (
                  icon && <img src={icon} alt="" className="size-3.5 shrink-0 object-contain" />
                )}
                {label}
              </button>
            )
          })}
        </div>
        <p className="mt-2 text-11 text-text-3">{t("filters.keywordCycle")}</p>
        {/* The match control above already states any/all, so only gaps in the tags are reported. */}
        <p className="mt-1 text-12 text-text-3 empty:hidden" role="status">
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
                : coverage === "partial"
                  ? t("filters.keywordPartial")
                  : null}
        </p>
      </div>
    </FilterDropdown>
  )
}
