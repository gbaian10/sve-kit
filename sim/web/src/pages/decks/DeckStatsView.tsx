import { ChevronDown, ChevronRight } from "lucide-react"
import { useState } from "react"
import { useTranslation } from "react-i18next"

import { CardTypeBadge } from "../../components/card/CardTypeBadge"
import { Segmented } from "../../components/ui/Segmented"
import {
  averageCost,
  costCurve,
  CURVE_SLOTS,
  TYPE_ORDER,
  typeCounts,
} from "../../domain/deck/stats"
import { useDeckEditor } from "./deck-editor"

type Scope = "main" | "evolve"

/**
 * Cost curve (0–8+), average cost and type counts of one deck at a time, the main deck or the
 * evolve deck. Evolve cards print no cost, so they count at their base card's cost, as the note
 * under the curve says.
 */
export function DeckStatsView() {
  const { t } = useTranslation()
  const { deck, facts } = useDeckEditor()
  const [open, setOpen] = useState(true)
  const [scope, setScope] = useState<Scope>("main")
  const entries = deck[scope]
  const costOf = (cardId: string) => facts?.chartCostOf(cardId) ?? null
  const infoOf = (cardId: string) => facts?.factsOf(cardId)
  const curve = costCurve(entries, costOf)
  const highest = Math.max(1, ...curve)
  const types = typeCounts(entries, infoOf)
  const total = entries.reduce((sum, entry) => sum + entry.count, 0)
  const average = averageCost(entries, costOf)
  return (
    <section className="flex flex-col gap-2" aria-label={t("decks.stats.title")}>
      <div className="flex items-center justify-between gap-2 px-1">
        <button
          type="button"
          aria-expanded={open}
          onClick={() => {
            setOpen((value) => !value)
          }}
          className="inline-flex h-11 items-center gap-1 rounded-button pr-2 text-13 font-semibold text-text-1 hover:bg-surface-2"
        >
          {open ? (
            <ChevronDown className="size-4" aria-hidden="true" />
          ) : (
            <ChevronRight className="size-4" aria-hidden="true" />
          )}
          {t("decks.stats.title")}
        </button>
        {open && (
          <Segmented
            size="sm"
            label={t("decks.stats.scope")}
            value={scope}
            options={[
              { value: "main", label: t("decks.section.main") },
              { value: "evolve", label: t("decks.section.evolve") },
            ]}
            onChange={setScope}
          />
        )}
      </div>
      {open && (
        <>
          <div className="flex items-end justify-between gap-2 px-1">
            <span className="text-12 font-semibold text-text-2">
              {t("decks.stats.curveCount", { count: total })}
            </span>
            {average !== null && (
              <span className="text-11 text-text-3 tabular-nums">
                {t("decks.stats.average", { value: average.toFixed(2) })}
              </span>
            )}
          </div>
          <ul
            aria-label={t("decks.stats.curve")}
            className="grid h-28 grid-cols-9 gap-1 rounded-button border border-border bg-surface-1 px-2 pt-1.5 pb-1"
          >
            {CURVE_SLOTS.map((slot, index) => {
              const count = curve[index] ?? 0
              const label = t("decks.stats.slot", { cost: slot, count })
              return (
                <li
                  key={slot}
                  className="flex h-full min-w-0 flex-col"
                  aria-label={label}
                  title={label}
                >
                  {/* The number has its own row, so the tallest bar never covers it. */}
                  <span className="h-4 shrink-0 pb-0.5 text-center text-11 leading-none text-text-2 tabular-nums">
                    {count > 0 ? count : ""}
                  </span>
                  <span className="flex min-h-0 flex-1 flex-col justify-end">
                    <span
                      className="w-full rounded-t-sm bg-accent"
                      style={{
                        height: `${String((count / highest) * 100)}%`,
                        minHeight: count ? 3 : 0,
                      }}
                    />
                  </span>
                  <span className="h-3.5 shrink-0 pt-0.5 text-center text-11 leading-none text-text-3 tabular-nums">
                    {slot}
                  </span>
                </li>
              )
            })}
          </ul>
          {scope === "evolve" && (
            <p className="px-1 text-11 text-text-3">{t("decks.stats.evolveCostNote")}</p>
          )}
          <ul className="flex flex-wrap gap-x-3 gap-y-1 px-1 text-12 text-text-2">
            {TYPE_ORDER.map((type) => (
              <li key={type} className="inline-flex items-center gap-1 tabular-nums">
                <CardTypeBadge code={type} label={t(`decks.types.${type}`)} decorative />
                {t("decks.stats.typeCount", { type: t(`decks.types.${type}`), count: types[type] })}
              </li>
            ))}
            {types.other > 0 && (
              <li className="tabular-nums">
                {t("decks.stats.typeCount", { type: t("decks.types.other"), count: types.other })}
              </li>
            )}
          </ul>
        </>
      )}
    </section>
  )
}
