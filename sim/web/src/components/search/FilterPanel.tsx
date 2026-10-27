import { ChevronDown } from "lucide-react"
import { type ReactNode, useId, useState } from "react"
import { useTranslation } from "react-i18next"

import { useMediaQuery } from "../../app/useMediaQuery"
import type { RestrictionStatus } from "../../data"
import {
  COST_CAP,
  QUERY_UNITS,
  type QueryState,
  REPRINT_MODES,
  STAT_CAP,
  toggleValue,
} from "../../domain/query/model"
import { REPRESENTATIVE_RARITIES } from "../../domain/representative"
import type { Region } from "../../domain/search"
import { Button } from "../ui/Button"
import { cn } from "../ui/cn"
import { Segmented } from "../ui/Segmented"
import { Switch } from "../ui/Switch"
import { Section, Toggle } from "./FilterControls"
import { StatValue } from "./StatValue"

export interface FilterPanelProps {
  /** The element id the search bar's filter button controls. */
  readonly id: string
  readonly open: boolean
  readonly query: QueryState
  readonly onChange: (next: QueryState) => void
  readonly edition: Region
  readonly onEditionChange: (edition: Region) => void
  readonly types?: ReactNode
  readonly locations?: ReactNode
  readonly facets: {
    readonly rarity: ReactNode
    readonly sets: ReactNode
    readonly traits: ReactNode
    readonly artists: ReactNode
    readonly keywords: ReactNode
  } | null
  readonly restrictions?: RestrictionStatus | undefined
  readonly resultTotal?: number | undefined
  readonly onClose?: () => void
  /** One column of titled groups for the cards page sidebar, instead of the panel grid. */
  readonly sidebar?: boolean
}

const EDITIONS = ["jp", "en"] as const
const range = (cap: number) => Array.from({ length: cap + 1 }, (_, value) => value)
const SELECT =
  "h-9 min-w-0 max-w-full rounded-control border border-border bg-surface-1 px-2 text-13 text-text-1 focus-visible:outline-offset-[-2px] disabled:cursor-not-allowed disabled:text-text-3"

function FilterGroup({
  title,
  collapsible,
  defaultOpen = false,
  children,
  desktopClassName,
}: {
  readonly title: string
  readonly collapsible: boolean
  readonly defaultOpen?: boolean
  readonly children: ReactNode
  readonly desktopClassName: string
}) {
  const [expanded, setExpanded] = useState(defaultOpen)
  const id = useId()
  const headingId = useId()
  const open = !collapsible || expanded
  return (
    <section
      aria-labelledby={headingId}
      className="min-w-0 rounded-control border border-border bg-surface-1 md:contents"
    >
      <h3
        id={headingId}
        className="rounded-t-control bg-surface-2 text-14 font-semibold text-text-1 md:hidden"
      >
        {collapsible ? (
          <button
            type="button"
            aria-expanded={open}
            aria-controls={id}
            onClick={() => {
              setExpanded(!expanded)
            }}
            className="flex min-h-11 w-full items-center justify-between gap-3 px-3 py-2 text-left"
          >
            {title}
            <ChevronDown aria-hidden className={cn("size-4 text-text-3", open && "rotate-180")} />
          </button>
        ) : (
          <span className="block border-b border-border px-3 py-2.5">{title}</span>
        )}
      </h3>
      <div id={id} hidden={!open} className={cn("space-y-4 p-3 md:space-y-0", desktopClassName)}>
        {children}
      </div>
    </section>
  )
}

// Filters apply at once; the condition chips remain the single place to clear everything.
export function FilterPanel({
  id,
  open,
  query,
  onChange,
  edition,
  onEditionChange,
  facets,
  types,
  locations,
  resultTotal,
  restrictions,
  onClose,
  sidebar = false,
}: FilterPanelProps) {
  const { t } = useTranslation()
  const collapsible = useMediaQuery("(width < 600px)")
  const merged = query.unit !== "printing"
  const cardMerged = query.unit === "card"
  const stat = (
    title: string,
    field: "cost" | "attack" | "defense",
    cap: number,
    values: readonly number[],
  ) => (
    <Section
      title={title}
      contentClassName="grid grid-cols-6 gap-1.5 md:grid-cols-4 lg:grid-cols-6"
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
          label={value === cap ? `${String(value)}+` : String(value)}
          className="h-11 px-1"
          onClick={() => {
            onChange({ ...query, [field]: toggleValue(values, value) })
          }}
        >
          <StatValue field={field} value={value} orMore={value === cap} />
        </Toggle>
      ))}
    </Section>
  )
  const displayBlock = (
    <>
      <div className="flex flex-wrap items-center gap-1.5">
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
            className="size-4 shrink-0 accent-accent focus-visible:outline-offset-[-2px]"
          />
          {t("filters.prFirst")}
        </label>
        <div className="flex w-full flex-wrap items-center gap-2">
          <span className="text-13 text-text-2">{t("filters.reprints")}</span>
          <Segmented
            label={t("filters.reprints")}
            size="sm"
            value={query.reprints}
            disabled={cardMerged}
            describedBy={cardMerged ? `${id}-reprints-hint` : undefined}
            options={REPRINT_MODES.map((mode) => ({
              value: mode,
              label: t(`filters.reprintModes.${mode}`),
            }))}
            onChange={(reprints) => {
              onChange({ ...query, reprints })
            }}
          />
        </div>
        {cardMerged && (
          <p id={`${id}-reprints-hint`} className="w-full text-12 text-text-3">
            {t("filters.reprintsMerged")}
          </p>
        )}
      </div>
    </>
  )
  const errataBlock = (
    <>
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
      <div className="flex items-center gap-2">
        <Switch
          size="sm"
          checked={query.restrictionsOnly}
          disabled={!restrictions || restrictions.state === "unknown"}
          label={t("filters.restrictionsOnly")}
          describedBy={`${id}-restriction-hint`}
          onChange={(checked) => {
            onChange({ ...query, restrictionsOnly: checked })
          }}
        />
        <div>
          <div className="text-13 text-text-1">{t("filters.restrictionsOnly")}</div>
          <p id={`${id}-restriction-hint`} className="text-12 text-text-3">
            {restrictions && restrictions.state !== "unknown"
              ? t(
                  restrictions.synthetic
                    ? "filters.restrictionsSyntheticDate"
                    : "filters.restrictionsDate",
                  { date: restrictions.onDate },
                )
              : t("filters.restrictionsUnknown")}
            {restrictions?.synthetic ? ` ${t("filters.restrictionsSynthetic")}` : ""}
            {restrictions?.state === "partial" ? ` ${t("filters.restrictionsPartial")}` : ""}
          </p>
        </div>
      </div>
    </>
  )
  const group = (title: string, children: ReactNode) => (
    <section className="space-y-3 border-b border-border pb-4">
      <h3 className="text-13 font-semibold text-text-1">{title}</h3>
      {children}
    </section>
  )
  if (sidebar) {
    return (
      <section id={id} aria-label={t("filters.panel")} className="flex min-w-0 flex-col gap-4">
        {group(
          t("filters.groups.basic"),
          <>
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
            {types}
            {locations}
          </>,
        )}
        {group(
          t("filters.groups.stats"),
          <>
            {stat(t("filters.cost"), "cost", COST_CAP, query.cost)}
            {stat(t("filters.attack"), "attack", STAT_CAP, query.attack)}
            {stat(t("filters.defense"), "defense", STAT_CAP, query.defense)}
          </>,
        )}
        {group(
          t("filters.groups.collection"),
          <>
            {facets?.rarity}
            {facets?.sets}
            {facets?.artists}
          </>,
        )}
        {group(
          t("filters.groups.effects"),
          <>
            {facets?.traits}
            {facets?.keywords}
          </>,
        )}
        {group(t("filters.groups.more"), errataBlock)}
        {group(t("filters.display"), displayBlock)}
      </section>
    )
  }
  return (
    <section
      id={id}
      hidden={!open}
      aria-label={t("filters.panel")}
      className="flex min-w-0 flex-col gap-3 rounded-block border border-border bg-surface-2 p-3 md:grid md:grid-cols-3 md:gap-0 md:bg-surface-1 md:p-0"
    >
      <FilterGroup
        title={t("filters.groups.basic")}
        collapsible={collapsible}
        defaultOpen
        desktopClassName="md:row-start-1 md:col-span-3 md:grid md:grid-cols-3 md:border-b md:border-border md:px-0 md:py-4"
      >
        <div className="md:px-4">
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
        </div>
        <div className="md:px-4">{types}</div>
        <div className="md:px-4">{locations}</div>
      </FilterGroup>
      <FilterGroup
        title={t("filters.groups.stats")}
        collapsible={collapsible}
        defaultOpen
        desktopClassName="md:row-start-2 md:col-span-3 md:grid md:grid-cols-3 md:border-b md:border-border md:px-0 md:py-4"
      >
        <div className="md:px-4">{stat(t("filters.cost"), "cost", COST_CAP, query.cost)}</div>
        <div className="md:px-4">{stat(t("filters.attack"), "attack", STAT_CAP, query.attack)}</div>
        <div className="md:px-4">
          {stat(t("filters.defense"), "defense", STAT_CAP, query.defense)}
        </div>
      </FilterGroup>
      <FilterGroup
        title={t("filters.groups.collection")}
        collapsible={collapsible}
        desktopClassName="md:contents"
      >
        <div className="md:col-span-3 md:row-start-3 md:border-b md:border-border md:p-4">
          {facets?.rarity}
        </div>
        <div className="space-y-4 md:col-start-2 md:row-start-4 md:p-4">
          {facets?.sets}
          {facets?.artists}
        </div>
        <div className="md:col-start-1 md:row-start-5 md:rounded-bl-block md:border-t md:border-border md:bg-surface-2 md:p-4">
          {errataBlock}
        </div>
      </FilterGroup>
      <FilterGroup
        title={t("filters.groups.effects")}
        collapsible={collapsible}
        desktopClassName="md:contents"
      >
        <div className="md:col-start-1 md:row-start-4 md:p-4">{facets?.traits}</div>
        <div className="md:col-start-3 md:row-start-4 md:p-4">{facets?.keywords}</div>
      </FilterGroup>
      <FilterGroup
        title={t("filters.display")}
        collapsible={collapsible}
        desktopClassName="md:row-start-5 md:col-span-2 md:col-start-2 md:rounded-br-block md:border-t md:border-border md:bg-surface-2 md:p-4"
      >
        <p className="mb-4 hidden text-12 font-semibold text-text-2 md:block">
          {t("filters.display")}
        </p>
        {displayBlock}
      </FilterGroup>
      <div className="sticky bottom-21 z-50 -mx-3 -mb-3 rounded-b-block border-t border-border bg-surface-1 p-3 md:hidden phone-landscape:bottom-0">
        <Button
          variant="primary"
          size="lg"
          className="w-full"
          disabled={resultTotal === undefined}
          onClick={onClose}
        >
          {resultTotal === undefined
            ? t("filters.updatingResults")
            : t("filters.showResults", { count: resultTotal })}
        </Button>
      </div>
    </section>
  )
}
