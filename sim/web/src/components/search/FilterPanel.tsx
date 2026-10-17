import type { ReactNode } from "react"
import { useTranslation } from "react-i18next"

import { COST_CAP, type QueryState, STAT_CAP, toggleValue } from "../../domain/query/model"
import type { Region } from "../../domain/search"
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
      <Section title={t("filters.rows")}>
        <Toggle
          pressed={query.unit === "card"}
          onClick={() => {
            onChange({ ...query, unit: "card" })
          }}
        >
          {t("filters.rowPerCard")}
        </Toggle>
        <Toggle
          pressed={query.unit === "printing"}
          onClick={() => {
            onChange({ ...query, unit: "printing" })
          }}
        >
          {t("filters.rowPerPrinting")}
        </Toggle>
      </Section>
    </section>
  )
}
