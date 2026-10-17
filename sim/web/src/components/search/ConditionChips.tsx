import type { ReactNode } from "react"
import { useTranslation } from "react-i18next"

import { COST_CAP, type QueryState, STAT_CAP, toggleClass } from "../../domain/query/model"
import { orderRarityTokens, UNRATED_RARITY } from "../../domain/rarityFilter"
import type { Region } from "../../domain/search"
import { orderTypeTokens } from "../../domain/typeFilter"
import { Chip } from "../ui/Chip"
import { prBadgeVisual, prStyleLabel, prStyleVisual, rarityBadgeVisual } from "./rarityVisuals"

export interface ConditionChipsProps {
  readonly query: QueryState
  readonly edition: Region
  readonly classLabel: (code: string) => string
  /** Vocabulary labels; each falls back to the code when the vocabulary has none. */
  readonly rarityLabel: (code: string) => string
  readonly typeLabel: (code: string) => string
  readonly setLabel: (code: string) => string
  readonly onChange: (next: QueryState) => void
  readonly onClear: () => void
}

interface Condition {
  readonly key: string
  readonly label: string
  readonly content?: ReactNode
  readonly without: QueryState
}

const STATS = [
  ["cost", COST_CAP],
  ["attack", STAT_CAP],
  ["defense", STAT_CAP],
] as const

// Every committed condition as one removable chip, in the panel's order; a parent selected as a
// whole (PR, Token, leader) is one chip, a partial selection one chip per sub-value. The row is
// the one place to clear everything, so it always carries "clear".
export function ConditionChips({
  query,
  edition,
  classLabel,
  rarityLabel,
  typeLabel,
  setLabel,
  onChange,
  onClear,
}: ConditionChipsProps) {
  const { t } = useTranslation()
  const sub = (parent: string, child: string) => t("filters.subChip", { parent, child })
  const typeChip = (token: string): Pick<Condition, "label"> => {
    const [code = "", child] = token.split(":")
    const parent = code === "token" ? t("typeFilter.token") : typeLabel(code)
    if (child === undefined) return { label: parent }
    return {
      label: sub(
        parent,
        code === "leader"
          ? child === "gold" || child === "white" || child === "unknown"
            ? t(`typeFilter.${child}`)
            : child
          : typeLabel(child),
      ),
    }
  }
  const rarityChip = (token: string): Pick<Condition, "label" | "content"> => {
    if (token === UNRATED_RARITY) return { label: t("rarityBadge.unrated") }
    if (token === "pr") {
      const label = rarityLabel(token)
      return { label, content: prBadgeVisual(label) }
    }
    if (token.startsWith("pr:")) {
      const style = token.slice("pr:".length)
      const label = prStyleLabel(t, style)
      const content = prStyleVisual(style, edition, label)
      return content === undefined ? { label } : { label, content }
    }
    const label = rarityLabel(token)
    const content = rarityBadgeVisual({ rarityCode: token, region: edition }, label)
    return content === undefined ? { label } : { label, content }
  }
  const conditions: Condition[] = [
    ...(query.text === ""
      ? []
      : [{ key: "text", label: query.text, without: { ...query, text: "" } }]),
    ...query.classes.map((code) => ({
      key: `class:${code}`,
      label: classLabel(code),
      without: toggleClass(query, code),
    })),
    ...STATS.flatMap(([field, cap]) =>
      query[field].map((value) => ({
        key: `${field}:${String(value)}`,
        label: t("filters.statChip", {
          section: t(`filters.${field}`),
          value: value === cap ? `${String(value)}+` : String(value),
        }),
        without: { ...query, [field]: query[field].filter((other) => other !== value) },
      })),
    ),
    ...orderRarityTokens(query.rarities).map((token) => ({
      key: `rarity:${token}`,
      ...rarityChip(token),
      without: { ...query, rarities: query.rarities.filter((other) => other !== token) },
    })),
    ...orderTypeTokens(query.types).map((token) => ({
      key: `type:${token}`,
      ...typeChip(token),
      without: { ...query, types: query.types.filter((other) => other !== token) },
    })),
    ...query.sets.map((code) => ({
      key: `set:${code}`,
      label: setLabel(code),
      without: { ...query, sets: query.sets.filter((other) => other !== code) },
    })),
  ]
  if (conditions.length === 0) return null
  return (
    <div
      role="group"
      aria-label={t("results.conditions")}
      className="flex flex-wrap items-center gap-2"
    >
      {conditions.map((condition) => (
        <Chip
          key={condition.key}
          removeLabel={t("results.removeCondition", { label: condition.label })}
          onRemove={() => {
            onChange(condition.without)
          }}
        >
          {condition.content ?? condition.label}
        </Chip>
      ))}
      <button
        type="button"
        onClick={onClear}
        className="h-8 rounded-pill px-2 text-13 font-semibold text-accent-text hover:bg-surface-2"
      >
        {t("results.clearConditions")}
      </button>
    </div>
  )
}
