import type { ReactNode } from "react"
import { useTranslation } from "react-i18next"

import { orderArtistTokens } from "../../domain/artistFilter"
import { orderLocationTokens } from "../../domain/deckLocation"
import { COST_CAP, type QueryState, STAT_CAP, toggleClass } from "../../domain/query/model"
import { orderRarityTokens, PREMIUM_RARITY, UNRATED_RARITY } from "../../domain/rarityFilter"
import type { Region } from "../../domain/search"
import { orderTypeTokens } from "../../domain/typeFilter"
import { Chip } from "../ui/Chip"
import { locationLabel } from "./facet-labels"
import { prBadgeVisual, prStyleLabel, prStyleVisual, rarityBadgeVisual } from "./rarityVisuals"

export interface ConditionChipsProps {
  readonly query: QueryState
  readonly edition: Region
  /** Vocabulary labels; each falls back to the code when the vocabulary has none. */
  readonly classLabel: (code: string) => string
  readonly rarityLabel: (code: string) => string
  readonly typeLabel: (code: string) => string
  readonly keywordLabel: (id: string) => string
  readonly traitLabel: (code: string) => string
  readonly artistLabel: (id: string) => string
  readonly setLabel: (code: string) => string
  /** The sub-values the panel offers under each parent (`pr`, `token`, `leader`), as tokens. */
  readonly groups: Readonly<Record<string, readonly string[]>>
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

// Every committed condition as one removable chip, in the panel's order. A group selected as a
// whole (its parent, or every sub-value the panel offers) is one chip whose × clears the whole
// group, the same reading the panel's chips use; a partial selection is one chip per sub-value.
// The row is the one place to clear everything, so it always carries "clear".
export function ConditionChips({
  query,
  edition,
  classLabel,
  rarityLabel,
  typeLabel,
  traitLabel,
  artistLabel,
  keywordLabel,
  setLabel,
  groups,
  onChange,
  onClear,
}: ConditionChipsProps) {
  const { t } = useTranslation()
  const parentOf = (token: string) =>
    Object.keys(groups).find((parent) => token === parent || token.startsWith(`${parent}:`))
  const whole = (selected: readonly string[], parent: string) => {
    const children = groups[parent] ?? []
    return (
      selected.includes(parent) ||
      (children.length > 0 && children.every((child) => selected.includes(child)))
    )
  }
  /** Collapses a whole group to its parent token, keeping the first member's position. */
  const collapse = (tokens: readonly string[]): string[] => {
    const result: string[] = []
    for (const token of tokens) {
      const parent = parentOf(token)
      const shown = parent !== undefined && whole(tokens, parent) ? parent : token
      if (!result.includes(shown)) result.push(shown)
    }
    return result
  }
  const without = (selected: readonly string[], token: string): string[] =>
    token in groups
      ? selected.filter((value) => value !== token && !value.startsWith(`${token}:`))
      : selected.filter((value) => value !== token)
  const sub = (parent: string, child: string) => t("filters.subChip", { parent, child })
  const locationChip = (token: string): Pick<Condition, "label"> => {
    const [code = "", child] = token.split(":")
    const parent = locationLabel(t, code)
    if (child === undefined) return { label: parent }
    return { label: sub(parent, code === "token" ? typeLabel(child) : locationLabel(t, token)) }
  }
  const rarityChip = (token: string): Pick<Condition, "label" | "content"> => {
    if (token === UNRATED_RARITY) return { label: t("rarityBadge.unrated") }
    if (token === PREMIUM_RARITY) return { label: t("rarityBadge.premium") }
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
    ...collapse(orderRarityTokens(query.rarities)).map((token) => ({
      key: `rarity:${token}`,
      ...rarityChip(token),
      without: { ...query, rarities: without(query.rarities, token) },
    })),
    ...(query.stampOnly
      ? [{ key: "stamp", label: t("filters.stampOnly"), without: { ...query, stampOnly: false } }]
      : []),
    ...orderTypeTokens(query.types).map((code) => ({
      key: `type:${code}`,
      label: typeLabel(code),
      without: { ...query, types: query.types.filter((other) => other !== code) },
    })),
    ...collapse(orderLocationTokens(query.locations)).map((token) => ({
      key: `loc:${token}`,
      ...locationChip(token),
      without: { ...query, locations: without(query.locations, token) },
    })),
    ...query.traits.map((code) => ({
      key: `trait:${code}`,
      label: traitLabel(code),
      without: { ...query, traits: query.traits.filter((other) => other !== code) },
    })),
    ...orderArtistTokens(query.artists).map((id) => ({
      key: `artist:${id}`,
      label: artistLabel(id),
      without: { ...query, artists: query.artists.filter((other) => other !== id) },
    })),
    ...Object.entries(query.mechanics).map(([id, state]) => ({
      key: `mechanic:${id}`,
      label: t(state === "has" ? "filters.keywordHas" : "filters.keywordNot", {
        label: keywordLabel(id),
      }),
      without: {
        ...query,
        mechanics: Object.fromEntries(
          Object.entries(query.mechanics).filter(([other]) => other !== id),
        ),
      },
    })),
    ...query.abilityTypes.map((code) => ({
      key: `ability:${code}`,
      label: t("filters.abilityTypeHas", { label: t(`filters.abilityTypeNames.${code}`) }),
      without: { ...query, abilityTypes: query.abilityTypes.filter((other) => other !== code) },
    })),
    ...(["classMechanisms", "collabMechanisms"] as const).flatMap((field) =>
      query[field].map((code) => ({
        key: `${field}:${code}`,
        label: t("filters.printedMechanismHas", {
          label: t(`filters.printedMechanismNames.${code}`),
        }),
        without: { ...query, [field]: query[field].filter((other) => other !== code) },
      })),
    ),
    ...query.sets.map((code) => ({
      key: `set:${code}`,
      label: setLabel(code),
      without: { ...query, sets: query.sets.filter((other) => other !== code) },
    })),
    ...(query.restrictionsOnly
      ? [
          {
            key: "restricted",
            label: t("filters.restrictionsOnly"),
            without: { ...query, restrictionsOnly: false },
          },
        ]
      : []),
    ...(query.serialOnly
      ? [
          {
            key: "serial",
            label: t("filters.serialOnly"),
            without: { ...query, serialOnly: false },
          },
        ]
      : []),
    ...(query.errataOnly
      ? [
          {
            key: "errata",
            label: t("filters.errataOnly"),
            without: { ...query, errataOnly: false },
          },
        ]
      : []),
    ...(query.reprints === "all"
      ? []
      : [
          {
            key: "reprints",
            label: t(`filters.reprintModes.${query.reprints}`),
            without: { ...query, reprints: "all" as const },
          },
        ]),
    // Merging cells is display, but the maintainer wants it removable from here too.
    ...(query.unit === "printing"
      ? []
      : [
          {
            key: "unit",
            label: t(`filters.displayUnit.${query.unit}`),
            without: { ...query, unit: "printing" as const },
          },
        ]),
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
