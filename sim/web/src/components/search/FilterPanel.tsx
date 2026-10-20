import type { ReactNode } from "react"
import { useTranslation } from "react-i18next"

import {
  COST_CAP,
  QUERY_UNITS,
  type QueryState,
  STAT_CAP,
  toggleValue,
} from "../../domain/query/model"
import { REPRESENTATIVE_RARITIES } from "../../domain/representative"
import type { Region } from "../../domain/search"
import { cn } from "../ui/cn"
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
const SELECT =
  "h-9 rounded-control border border-border bg-surface-1 px-2 text-13 text-text-1 disabled:cursor-not-allowed disabled:text-text-3"

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
  const merged = query.unit !== "printing"
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
      <div className="flex items-center gap-2">
        <Switch
          size="sm"
          checked={query.errataOnly}
          label={t("filters.errataOnly")}
          describedBy={`${id}-errata-hint`}
          onChange={(checked) => {
            onChange({ ...query, errataOnly: checked })
          }}
        />
        <div>
          <div className="text-13 text-text-1">{t("filters.errataOnly")}</div>
          <p id={`${id}-errata-hint`} className="text-12 text-text-3">
            {t("filters.errataOnlyHint")}
          </p>
        </div>
      </div>
      {/* Display, not a filter: how many cells a card gets, and which printing a merged cell shows
          (#781). The representative choice only applies to merged cells, so it is disabled, not
          hidden, while every printing is listed: the URL keeps it for the next switch. */}
      <Section title={t("filters.display")}>
        <select
          aria-label={t("filters.display")}
          value={query.unit}
          className={SELECT}
          onChange={(event) => {
            const unit = QUERY_UNITS.find((unit) => unit === event.target.value)
            if (unit) onChange({ ...query, unit })
          }}
        >
          {QUERY_UNITS.map((unit) => (
            <option key={unit} value={unit}>
              {t(`filters.displayUnit.${unit}`)}
            </option>
          ))}
        </select>
        <select
          aria-label={t("filters.representative")}
          value={query.representative}
          disabled={!merged}
          className={SELECT}
          onChange={(event) => {
            const representative = REPRESENTATIVE_RARITIES.find(
              (rarity) => rarity === event.target.value,
            )
            if (representative) onChange({ ...query, representative })
          }}
        >
          {REPRESENTATIVE_RARITIES.map((rarity) => (
            <option key={rarity} value={rarity}>
              {t(`filters.representativeRarity.${rarity}`)}
            </option>
          ))}
        </select>
        <label
          className={cn(
            "flex h-9 items-center gap-2 px-1 text-13",
            merged ? "cursor-pointer text-text-1" : "cursor-not-allowed text-text-3",
          )}
        >
          <input
            type="checkbox"
            checked={query.prFirst}
            disabled={!merged}
            onChange={(event) => {
              onChange({ ...query, prFirst: event.target.checked })
            }}
            className="size-4 shrink-0 accent-accent"
          />
          {t("filters.prFirst")}
        </label>
      </Section>
    </section>
  )
}
