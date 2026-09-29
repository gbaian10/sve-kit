import { useMemo, useState } from "react"
import { useTranslation } from "react-i18next"

import type { FilterOptions } from "../../data"
import { classStyle } from "../../domain/classes"
import {
  type CostRange,
  DEFAULT_QUERY,
  type MechanicState,
  NEUTRAL_CLASS,
  QUERY_UNITS,
  type QueryState,
} from "../../domain/query/model"
import type { TextLang } from "../../domain/search"
import { CLASS_ICON } from "../card/classIcons"
import { Button } from "../ui/Button"
import { cn } from "../ui/cn"
import { Dialog } from "../ui/Dialog"
import { Segmented } from "../ui/Segmented"
import { Switch } from "../ui/Switch"
import { withoutMechanic } from "./chips"

export interface FilterSheetProps {
  readonly open: boolean
  readonly onClose: () => void
  /** The applied state the draft starts from. */
  readonly applied: QueryState
  readonly onApply: (state: QueryState) => void
  readonly options: FilterOptions
  readonly classes: readonly { readonly code: string; readonly label: string }[]
  readonly label: (kind: "type" | "rarity", code: string) => string
  readonly uiLang: TextLang
  /** Result count for a draft, for the "show N" button. */
  readonly count: (state: QueryState) => number
  readonly coverage: { readonly annotated: number; readonly total: number }
}

const COSTS = [0, 1, 2, 3, 4, 5, 6, 7] as const
type MechanicChoice = MechanicState | "any"
const MECHANIC_NEXT: Record<MechanicChoice, MechanicChoice> = {
  any: "has",
  has: "not",
  not: "any",
}
const MECHANIC_GLYPH: Record<MechanicChoice, string> = { any: "·", has: "✓", not: "✕" }
const mechanicChoice = (state: QueryState, id: string): MechanicChoice =>
  state.mechanics[id] ?? "any"

function toggle(list: readonly string[], code: string): string[] {
  return list.includes(code) ? list.filter((item) => item !== code) : [...list, code].sort()
}

/** A contiguous cost range shown as chips: the selected chips run from min to max. */
function costChipsSelected(range: CostRange, value: number): boolean {
  if (range.min === undefined && range.max === undefined) return false
  return (range.min ?? 0) <= value && value <= (range.max ?? 7)
}

function toggleCost(range: CostRange, value: number): CostRange {
  const min = range.min ?? (range.max === undefined ? undefined : 0)
  const max = range.max ?? (range.min === undefined ? undefined : 7)
  if (min === undefined || max === undefined) return { min: value, max: value }
  if (value < min) return { min: value, max }
  if (value > max) return { min, max: value }
  // Tapping inside the range shrinks it from the nearer end; tapping the only chip clears it.
  if (min === max) return {}
  return value - min <= max - value
    ? { min: value + 1 > max ? max : value + 1, max }
    : { min, max: value - 1 }
}

function Chip({
  selected,
  onClick,
  children,
  className,
}: {
  readonly selected: boolean
  readonly onClick: () => void
  readonly children: React.ReactNode
  readonly className?: string
}) {
  return (
    <button
      type="button"
      aria-pressed={selected}
      onClick={onClick}
      className={cn(
        "flex h-9 min-w-9 items-center justify-center gap-1 rounded-pill border px-3 text-13 font-semibold",
        selected
          ? "border-text-1 bg-surface-3 text-text-1 ring-1 ring-text-1 ring-inset"
          : "border-border bg-surface-1 text-text-2 hover:bg-surface-2",
        className,
      )}
    >
      {children}
    </button>
  )
}

function Section({
  title,
  hint,
  children,
}: {
  readonly title: string
  readonly hint?: string
  readonly children: React.ReactNode
}) {
  return (
    <section className="flex flex-col gap-2">
      <h3 className="text-13 font-semibold text-text-2">
        {title}
        {hint !== undefined && <span className="ml-1 text-12 font-normal text-text-3">{hint}</span>}
      </h3>
      {children}
    </section>
  )
}

// Design 05a: a draft the bottom buttons apply ("show N" counts live), closing keeps nothing.
export function FilterSheet({
  open,
  onClose,
  applied,
  onApply,
  options,
  classes,
  label,
  uiLang,
  count,
  coverage,
}: FilterSheetProps) {
  const { t } = useTranslation()
  const [draft, setDraft] = useState<QueryState>(applied)
  const [setQuery, setSetQuery] = useState("")
  // Reopening starts from what is applied now, not from an old draft.
  const [openedFor, setOpenedFor] = useState<QueryState | null>(null)
  if (open && openedFor !== applied) {
    setOpenedFor(applied)
    setDraft(applied)
  }
  const total = useMemo(() => count(draft), [count, draft])
  const patch = (next: Partial<QueryState>) => {
    setDraft((current) => ({ ...current, ...next }))
  }
  const visibleSets = options.sets.filter((set) =>
    setQuery.trim() === "" ? true : set.code.toLowerCase().includes(setQuery.trim().toLowerCase()),
  )
  const costLabel = (value: number) => (value === 7 ? t("filters.costMax") : String(value))
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={t("filters.title")}
      className="h-[92dvh] md:h-auto"
      footer={
        <div className="flex gap-2.5">
          <Button
            size="lg"
            onClick={() => {
              setDraft(DEFAULT_QUERY)
            }}
          >
            {t("filters.reset")}
          </Button>
          <Button
            size="lg"
            variant="primary"
            className="flex-1"
            onClick={() => {
              onApply({
                ...draft,
                text: applied.text,
                ...(applied.view === undefined ? {} : { view: applied.view }),
              })
            }}
          >
            {t("filters.show", { count: total })}
          </Button>
        </div>
      }
    >
      <div className="flex flex-col gap-5 pt-1">
        <Section title={t("filters.classes")}>
          <div className="flex flex-wrap gap-1.5">
            {classes.map((item) => {
              const style = classStyle(item.code === NEUTRAL_CLASS ? null : item.code)
              return (
                <Chip
                  key={item.code}
                  selected={draft.classes.includes(item.code)}
                  onClick={() => {
                    patch({ classes: toggle(draft.classes, item.code) })
                  }}
                >
                  <img src={CLASS_ICON[style]} alt="" className="size-5" />
                  {item.label}
                </Chip>
              )
            })}
          </div>
        </Section>
        <Section title={t("filters.cost")}>
          <div className="flex flex-wrap gap-1.5">
            {COSTS.map((value) => (
              <Chip
                key={value}
                selected={costChipsSelected(draft.cost, value)}
                onClick={() => {
                  patch({ cost: toggleCost(draft.cost, value) })
                }}
                className="min-w-11 px-0 tabular-nums"
              >
                {costLabel(value)}
              </Chip>
            ))}
          </div>
        </Section>
        <Section title={t("filters.types")}>
          <div className="flex flex-wrap gap-1.5">
            {options.types.map((code) => (
              <Chip
                key={code}
                selected={draft.types.includes(code)}
                onClick={() => {
                  patch({ types: toggle(draft.types, code) })
                }}
              >
                {label("type", code)}
              </Chip>
            ))}
          </div>
        </Section>
        <Section title={t("filters.mechanics")} hint={t("filters.mechanicsHint")}>
          <p className="text-12 text-text-3">
            {t("filters.mechanicsCoverage", {
              annotated: coverage.annotated,
              total: coverage.total,
            })}
          </p>
          <div className="flex flex-col gap-1.5">
            {options.keywords.map((keyword) => {
              const state = mechanicChoice(draft, keyword.id)
              const glyph = MECHANIC_GLYPH[state]
              return (
                <button
                  key={keyword.id}
                  type="button"
                  onClick={() => {
                    const next = MECHANIC_NEXT[state]
                    patch({
                      mechanics:
                        next === "any"
                          ? withoutMechanic(draft.mechanics, keyword.id)
                          : { ...draft.mechanics, [keyword.id]: next },
                    })
                  }}
                  className={cn(
                    "flex h-11 items-center gap-3 rounded-control border px-3 text-14",
                    state === "any" ? "border-border bg-surface-1" : "bg-surface-3",
                    state === "has"
                      ? "border-success"
                      : state === "not"
                        ? "border-border-strong"
                        : "",
                  )}
                >
                  <span
                    aria-hidden="true"
                    className={cn(
                      "flex size-6 items-center justify-center rounded-full text-13 font-bold",
                      state === "has"
                        ? "bg-success-soft text-success"
                        : state === "not"
                          ? "bg-surface-2 text-text-3"
                          : "text-text-3",
                    )}
                  >
                    {glyph}
                  </span>
                  <span className="flex-1 text-left font-semibold">{keyword.name(uiLang)}</span>
                  <span className="text-12 text-text-3">
                    {state === "has"
                      ? t("filters.mechanicHas")
                      : state === "not"
                        ? t("filters.mechanicNot")
                        : t("filters.mechanicAny")}
                  </span>
                </button>
              )
            })}
          </div>
        </Section>
        <Section title={t("filters.sets")}>
          <input
            type="search"
            aria-label={t("filters.setsSearch")}
            placeholder={t("filters.setsSearch")}
            value={setQuery}
            onChange={(event) => {
              setSetQuery(event.target.value)
            }}
            className="h-10 rounded-control border border-border bg-surface-1 px-3 text-14 placeholder:text-text-3 focus:outline-none focus-visible:outline-2"
          />
          <div className="flex flex-wrap gap-1.5">
            {visibleSets.map((set) => (
              <Chip
                key={set.code}
                selected={draft.sets.includes(set.code)}
                onClick={() => {
                  patch({ sets: toggle(draft.sets, set.code) })
                }}
              >
                {set.code.toUpperCase()}
              </Chip>
            ))}
          </div>
        </Section>
        <Section title={t("filters.rarities")}>
          <div className="flex flex-wrap gap-1.5">
            {options.rarities.map((code) => (
              <Chip
                key={code}
                selected={draft.rarities.includes(code)}
                onClick={() => {
                  patch({ rarities: toggle(draft.rarities, code) })
                }}
              >
                {label("rarity", code)}
              </Chip>
            ))}
          </div>
        </Section>
        <Switch
          label={t("filters.altArtOnly")}
          checked={draft.altArtOnly}
          onChange={(altArtOnly) => {
            patch({ altArtOnly })
          }}
        />
        <Section title={t("filters.unit")}>
          <Segmented
            label={t("filters.unit")}
            options={QUERY_UNITS.map((unit) => ({
              value: unit,
              label: t(`filters.units.${unit}`),
            }))}
            value={draft.unit}
            onChange={(unit) => {
              patch({ unit })
            }}
          />
        </Section>
      </div>
    </Dialog>
  )
}
