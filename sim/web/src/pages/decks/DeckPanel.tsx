import { ArrowLeft, Share2 } from "lucide-react"
import { useState } from "react"
import { useTranslation } from "react-i18next"

import { CLASS_ICON } from "../../components/card/classIcons"
import { Button } from "../../components/ui/Button"
import { cn } from "../../components/ui/cn"
import { Link } from "../../components/ui/Link"
import { Segmented } from "../../components/ui/Segmented"
import type { ImageIndex, RestrictionStatus } from "../../data"
import { classStyle } from "../../domain/classes"
import { EVOLVE_MAX, MAIN_MAX, MAIN_MIN } from "../../domain/deck/construction"
import { totalCount } from "../../domain/deck/model"
import { groupEntries, type GroupMode } from "../../domain/deck/stats"
import type { DeckSection } from "../../domain/deck/types"
import { useDeckEditor } from "./deck-editor"
import { useDeckLabels } from "./deck-labels"
import { DeckProblems } from "./DeckProblems"
import { DeckRow, LeaderRow } from "./DeckRow"
import { DeckStatsView } from "./DeckStatsView"
import { ExportDialog } from "./ExportDialog"
import { PrintingPicker, type PrintingTarget } from "./PrintingPicker"

export function CountPill({
  label,
  count,
  min,
  max,
  compactInRail = false,
}: {
  readonly label: string
  readonly count: number
  readonly min: number
  readonly max: number
  /** In the landscape-phone rail only the count shows; the label stays for screen readers. */
  readonly compactInRail?: boolean
}) {
  const { t } = useTranslation()
  return (
    <span
      className={cn(
        "inline-flex h-7 items-center gap-1 rounded-pill border px-2.5 text-12 font-semibold tabular-nums",
        count > max
          ? "border-danger bg-danger-soft text-danger"
          : count < min
            ? "border-border bg-surface-1 text-text-2"
            : "border-success bg-success-soft text-success",
      )}
    >
      <span className={cn(compactInRail && "phone-landscape:sr-only")}>{label}</span>
      <span>
        <span className="text-13">{count}</span>
        <span className={cn("text-text-3", compactInRail && "phone-landscape:hidden")}>
          {min === 0 ? t("decks.limitMax", { max }) : t("decks.limitRange", { min, max })}
        </span>
      </span>
    </span>
  )
}

/** The deck: name and edition, basis and counts, leader, problems, stats and the three lists. */
export function DeckPanel({
  images,
  restrictionStatus,
}: {
  readonly images: ImageIndex | undefined
  readonly restrictionStatus: RestrictionStatus | undefined
}) {
  const { t } = useTranslation()
  const editor = useDeckEditor()
  const labels = useDeckLabels()
  const { deck, facts } = editor
  const [groupMode, setGroupMode] = useState<GroupMode>("type")
  const [picking, setPicking] = useState<PrintingTarget | null>(null)
  const [exporting, setExporting] = useState(false)
  const infoOf = (cardId: string) => facts?.factsOf(cardId)
  const mainCount = totalCount(deck.main)
  const evolveCount = totalCount(deck.evolve)
  const style = classStyle(deck.basis.kind === "class" ? deck.basis.classCode : null)
  const groupLabel = (key: string | null) =>
    key === null
      ? t("decks.stats.noCost")
      : groupMode === "type"
        ? t(`decks.types.${key as "follower"}`)
        : t("decks.stats.costSlot", { cost: key })
  const section = (key: DeckSection, title: string, limit: string) => {
    const entries = deck[key]
    if (key === "side" && entries.length === 0) return null
    const rows = (list: typeof entries) => (
      <ul className="flex flex-col">
        {list.map((entry) => (
          <DeckRow
            key={entry.cardId}
            entry={entry}
            section={key}
            images={images}
            onPickPrinting={(cardId) => {
              setPicking({ cardId, section: key })
            }}
          />
        ))}
      </ul>
    )
    return (
      <section className="flex flex-col gap-1" aria-label={title}>
        <h3 className="flex items-baseline justify-between px-1 text-13 font-semibold text-text-2">
          <span>{title}</span>
          <span className="text-text-3 tabular-nums">{limit}</span>
        </h3>
        {entries.length === 0 ? (
          <p className="rounded-button border border-dashed border-border-strong px-3 py-4 text-center text-12 text-text-3">
            {key === "main" ? t("decks.emptyMainHint") : t("decks.emptyEvolveHint")}
          </p>
        ) : key === "side" ? (
          rows(entries)
        ) : (
          groupEntries(entries, infoOf, groupMode).map((group) => (
            <div key={String(group.key)} className="flex flex-col">
              <div className="flex items-baseline justify-between px-1 pt-1 text-11 text-text-3">
                <span>{groupLabel(group.key)}</span>
                <span className="tabular-nums">{group.count}</span>
              </div>
              {rows(group.entries)}
            </div>
          ))
        )}
      </section>
    )
  }
  return (
    <div className="flex flex-col gap-3" data-deck-panel="">
      <header className="flex flex-col gap-2">
        <div className="flex items-center gap-2">
          <Link
            to="/decks"
            className="flex size-9 shrink-0 items-center justify-center rounded-button text-text-2 no-underline hover:bg-surface-2"
            aria-label={t("decks.backToList")}
            title={t("decks.backToList")}
          >
            <ArrowLeft className="size-4.5" aria-hidden="true" />
          </Link>
          <input
            value={deck.name}
            onChange={(event) => {
              editor.rename(event.target.value)
            }}
            aria-label={t("decks.nameLabel")}
            className="h-10 min-w-0 flex-1 rounded-button border border-transparent bg-transparent px-2 text-17 font-bold text-text-1 hover:border-border focus:border-border-strong focus:bg-surface-1"
          />
          {/* The edition keeps the right end of the name row, so the row below fits one line. */}
          <span className="inline-flex h-7 shrink-0 items-center rounded-pill border border-border bg-surface-1 px-2 text-11 font-semibold text-text-2">
            {t(`options.edition.${deck.edition}`)}
          </span>
        </div>
        <div className="flex flex-wrap items-center gap-1.5 px-1">
          <span
            data-class={style}
            className="inline-flex h-7 items-center gap-1.5 rounded-pill border border-border bg-surface-1 pr-2.5 pl-1.5 text-12 font-semibold text-class-current"
          >
            <img src={CLASS_ICON[style]} alt="" className="size-4" />
            {labels.basisLabel(deck.basis)}
          </span>
          <CountPill label={t("decks.pill.main")} count={mainCount} min={MAIN_MIN} max={MAIN_MAX} />
          <CountPill label={t("decks.pill.evolve")} count={evolveCount} min={0} max={EVOLVE_MAX} />
        </div>
      </header>
      {facts === null ? (
        <p
          role="status"
          className="rounded-button border border-border bg-surface-1 px-3 py-4 text-center text-13 text-text-2"
        >
          {t("results.dependenciesPending")}
        </p>
      ) : (
        <>
          <LeaderRow
            images={images}
            onPickPrinting={(cardId) => {
              setPicking({ cardId, section: "leader" })
            }}
          />
          <DeckProblems restrictionStatus={restrictionStatus} />
          <DeckStatsView />
          <div className="flex items-center justify-between gap-2 px-1">
            <span className="text-13 font-semibold text-text-1">{t("decks.listTitle")}</span>
            <Segmented
              size="sm"
              label={t("decks.groupBy")}
              value={groupMode}
              options={[
                { value: "type", label: t("decks.groupByType") },
                { value: "cost", label: t("decks.groupByCost") },
              ]}
              onChange={setGroupMode}
            />
          </div>
          {section(
            "main",
            t("decks.section.main"),
            t("decks.sectionCount", {
              count: mainCount,
              limit: t("decks.limitRange", { min: MAIN_MIN, max: MAIN_MAX }),
            }),
          )}
          {section(
            "evolve",
            t("decks.section.evolve"),
            t("decks.sectionCount", {
              count: evolveCount,
              limit: t("decks.limitMax", { max: EVOLVE_MAX }),
            }),
          )}
          {section(
            "side",
            t("decks.section.side"),
            t("decks.sideCount", { count: totalCount(deck.side) }),
          )}
        </>
      )}
      <Button
        variant="primary"
        className="mx-1"
        onClick={() => {
          setExporting(true)
        }}
      >
        <Share2 className="size-4" aria-hidden="true" />
        {t("decks.export")}
      </Button>
      <p className="px-1 text-11 text-text-3">{t("decks.savedLocally")}</p>
      <ExportDialog
        images={images}
        open={exporting}
        onClose={() => {
          setExporting(false)
        }}
      />
      <PrintingPicker
        target={picking}
        images={images}
        onClose={() => {
          setPicking(null)
        }}
      />
    </div>
  )
}
