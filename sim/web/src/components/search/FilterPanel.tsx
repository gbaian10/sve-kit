import { type ReactNode, useId } from "react"
import { useTranslation } from "react-i18next"

import { COST_CAP, type QueryState, STAT_CAP, toggleValue } from "../../domain/query/model"
import type { Region } from "../../domain/search"
import { Switch } from "../ui/Switch"
import { Section, Toggle } from "./FilterControls"

export interface FilterPanelProps {
  /** The element id the search bar's filter button controls. */
  readonly id: string
  readonly open: boolean
  readonly query: QueryState
  readonly onChange: (next: QueryState) => void
  readonly edition: Region
  readonly onEditionChange: (edition: Region) => void
  /** The facet rows (rarity, type, set) rendered between the stats and the row unit. */
  readonly facets: ReactNode
}

const EDITIONS = ["jp", "en"] as const
const range = (cap: number) => Array.from({ length: cap + 1 }, (_, value) => value)

// Every button applies at once (the maintainer chose the reference site's behaviour over the
// drafted "apply" step); the panel only renders the committed state it is given. The class row
// lives outside the panel so it stays visible while this is collapsed, and the condition chip row
// below the panel is the one place that clears everything.
export function FilterPanel({
  id,
  open,
  query,
  onChange,
  edition,
  onEditionChange,
  facets,
}: FilterPanelProps) {
  const { t } = useTranslation()
  const hintId = useId()
  const stat = (
    title: string,
    field: "cost" | "attack" | "defense",
    cap: number,
    values: readonly number[],
  ) => (
    <Section
      title={title}
      onClear={
        values.length > 0
          ? () => {
              onChange({ ...query, [field]: [] })
            }
          : undefined
      }
    >
      {range(cap).map((value) => (
        <Toggle
          key={value}
          pressed={values.includes(value)}
          onClick={() => {
            onChange({ ...query, [field]: toggleValue(values, value) })
          }}
        >
          {value === cap ? `${String(value)}+` : String(value)}
        </Toggle>
      ))}
    </Section>
  )
  return (
    <section
      id={id}
      hidden={!open}
      aria-label={t("filters.panel")}
      className="flex flex-col gap-3 rounded-block border border-border bg-surface-1 p-3"
    >
      <Section title={t("filters.edition")}>
        {EDITIONS.map((code) => (
          <Toggle
            key={code}
            pressed={edition === code}
            onClick={() => {
              if (edition !== code) onEditionChange(code)
            }}
          >
            {t(`options.edition.${code}`)}
          </Toggle>
        ))}
      </Section>
      {stat(t("filters.cost"), "cost", COST_CAP, query.cost)}
      {stat(t("filters.attack"), "attack", STAT_CAP, query.attack)}
      {stat(t("filters.defense"), "defense", STAT_CAP, query.defense)}
      {facets}
      {/* Display, not a filter: on = one row per card (`unit=card`), off = one per printing. */}
      <div className="flex items-start gap-2">
        <Switch
          size="sm"
          label={t("filters.onePerCard")}
          describedBy={hintId}
          checked={query.unit === "card"}
          onChange={(checked) => {
            onChange({ ...query, unit: checked ? "card" : "printing" })
          }}
        />
        <div className="flex min-w-0 flex-col justify-center py-2.5 leading-tight">
          <span aria-hidden="true" className="text-13 text-text-1">
            {t("filters.onePerCard")}
          </span>
          <span id={hintId} className="text-12 text-text-3">
            {t("filters.onePerCardHint")}
          </span>
        </div>
      </div>
    </section>
  )
}
