import { type ReactNode, useId } from "react"
import { useTranslation } from "react-i18next"

import {
  COST_CAP,
  type QueryState,
  STAT_CAP,
  toggleClass,
  toggleValue,
} from "../../domain/query/model"
import type { Region } from "../../domain/search"
import { Button } from "../ui/Button"
import { cn } from "../ui/cn"
import { ClassQuickBar, type QuickBarClass } from "./ClassQuickBar"

export interface FilterPanelProps {
  /** The element id the search bar's filter button controls. */
  readonly id: string
  readonly open: boolean
  readonly query: QueryState
  readonly onChange: (next: QueryState) => void
  readonly edition: Region
  readonly onEditionChange: (edition: Region) => void
  readonly classes: readonly QuickBarClass[]
  /** The dropdown facets (rarity, type, set) that keep their checklist form in this version. */
  readonly facets: ReactNode
  /** Shown while any condition is set; clears every condition. */
  readonly onClearAll?: (() => void) | undefined
}

const EDITIONS = ["jp", "en"] as const
const range = (cap: number) => Array.from({ length: cap + 1 }, (_, value) => value)

function Toggle({
  pressed,
  onClick,
  children,
}: {
  readonly pressed: boolean
  readonly onClick: () => void
  readonly children: ReactNode
}) {
  return (
    <button
      type="button"
      aria-pressed={pressed}
      onClick={onClick}
      className={cn(
        "flex h-9 min-w-9 items-center justify-center rounded-control border px-2.5 text-13 font-semibold whitespace-nowrap tabular-nums",
        pressed
          ? "border-text-1 bg-surface-3 text-text-1 ring-1 ring-text-1 ring-inset"
          : "border-border bg-surface-1 text-text-2 hover:bg-surface-2",
      )}
    >
      {children}
    </button>
  )
}

function Section({ title, children }: { readonly title: string; readonly children: ReactNode }) {
  const id = useId()
  return (
    <div role="group" aria-labelledby={id} className="flex flex-col gap-1.5">
      <p id={id} className="text-12 font-semibold text-text-2">
        {title}
      </p>
      <div className="flex flex-wrap gap-1.5">{children}</div>
    </div>
  )
}

// Every button applies at once (the maintainer chose the reference site's behaviour over the
// drafted "apply" step); the panel only renders the committed state it is given.
export function FilterPanel({
  id,
  open,
  query,
  onChange,
  edition,
  onEditionChange,
  classes,
  facets,
  onClearAll,
}: FilterPanelProps) {
  const { t } = useTranslation()
  const stat = (
    title: string,
    field: "cost" | "attack" | "defense",
    cap: number,
    values: readonly number[],
  ) => (
    <Section title={title}>
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
      {onClearAll && (
        <div className="flex justify-end">
          <Button size="sm" variant="ghost" className="text-accent-text" onClick={onClearAll}>
            {t("filters.clearAll")}
          </Button>
        </div>
      )}
      {classes.length > 0 && (
        <ClassQuickBar
          label={t("search.classes")}
          classes={classes}
          selected={query.classes}
          onToggle={(code) => {
            onChange(toggleClass(query, code))
          }}
        />
      )}
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
      <div className="flex flex-wrap items-start gap-2">{facets}</div>
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
