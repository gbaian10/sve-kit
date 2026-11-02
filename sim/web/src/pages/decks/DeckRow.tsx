import { Crown, Layers } from "lucide-react"
import { useTranslation } from "react-i18next"

import { CardImage } from "../../components/card/CardImage"
import { RestrictionBadge } from "../../components/card/RestrictionBadge"
import { costIcon, costIconNeedsNumber } from "../../components/card/symbolIcons"
import { cn } from "../../components/ui/cn"
import type { ImageIndex } from "../../data"
import type { DeckEntry, DeckSection } from "../../domain/deck/types"
import { displayName } from "../../domain/nameDisplay"
import { CountStepper } from "./CountStepper"
import { useDeckEditor } from "./deck-editor"

function CostGem({ cost }: { readonly cost: number | null }) {
  const { t } = useTranslation()
  if (cost === null)
    return (
      <span
        aria-hidden="true"
        className="inline-block size-6 shrink-0 text-center text-12 text-text-3"
      >
        —
      </span>
    )
  // The crystal shows 0–10 itself; other costs get the number beside it, as on the card page.
  return (
    <span className="inline-flex shrink-0 items-center gap-0.5" title={t("cardPage.stats.cost")}>
      <img src={costIcon(cost)} alt={t("cardPage.stats.cost")} className="size-6" />
      {costIconNeedsNumber(cost) && <b className="text-11 tabular-nums">{cost}</b>}
    </span>
  )
}

/**
 * One line of the deck list: cost, a small image, the name (opens the card), the card number
 * (opens the printing picker), attack/defense, the restriction badge and the stepper.
 */
export function DeckRow({
  entry,
  section,
  images,
  onPickPrinting,
}: {
  readonly entry: DeckEntry
  readonly section: DeckSection
  readonly images: ImageIndex | undefined
  readonly onPickPrinting: (cardId: string) => void
}) {
  const { t } = useTranslation()
  const editor = useDeckEditor()
  const facts = editor.facts?.factsOf(entry.cardId)
  const summary = editor.facts?.summaryOf(entry.cardId, entry.printingId)
  const status = editor.statusOf(entry.cardId)
  const name = summary
    ? displayName(summary.name, editor.uiLanguage, editor.nameDisplay)
    : { primary: { text: facts?.name ?? entry.cardId, lang: "ja" as const } }
  const restrictions = editor.restrictions.get(entry.cardId)
  const failing = editor.problems.some(
    (item) => item.severity === "error" && item.cardIds.includes(entry.cardId),
  )
  const otherPrintings = (facts?.printings.length ?? 1) - 1
  return (
    <li
      data-deck-card={entry.cardId}
      className={cn(
        "flex items-center gap-2 rounded-button px-1 py-1",
        editor.highlight === entry.cardId && "ring-2 ring-accent",
        failing && "bg-danger-soft",
      )}
    >
      <CostGem cost={facts?.cost ?? null} />
      {summary ? (
        <CardImage
          summary={summary}
          name={name.primary}
          images={images}
          alt="decorative"
          size="small"
          className="w-8"
        />
      ) : (
        <span className="block aspect-[63/88] w-8 shrink-0 rounded-card bg-surface-2" />
      )}
      <span className="flex min-w-0 flex-1 flex-col leading-tight">
        <button
          type="button"
          disabled={!summary}
          onClick={() => {
            if (summary) editor.openCard(summary)
          }}
          className="min-w-0 truncate text-start text-13 font-medium hover:underline disabled:no-underline"
          lang={name.primary.lang}
          title={name.primary.text}
        >
          {name.primary.text}
        </button>
        <span className="flex min-w-0 items-center gap-1.5 text-11 text-text-3">
          <button
            type="button"
            onClick={() => {
              onPickPrinting(entry.cardId)
            }}
            aria-label={t("decks.pickPrintingNamed", { name: name.primary.text })}
            title={t("decks.pickPrinting")}
            className="inline-flex shrink-0 items-center gap-1 rounded-badge px-1 whitespace-nowrap tabular-nums hover:bg-surface-2 hover:text-text-1"
          >
            <Layers className="size-3" aria-hidden="true" />
            {summary?.cardNo ?? entry.cardId}
            {otherPrintings > 0 && <span>+{otherPrintings}</span>}
          </button>
          {facts?.typeCode === "follower" && facts.attack !== null && facts.defense !== null && (
            <span className="tabular-nums">
              {facts.attack}/{facts.defense}
            </span>
          )}
          <RestrictionBadge restrictions={restrictions} />
        </span>
      </span>
      {section === "side" ? (
        <button
          type="button"
          onClick={() => {
            editor.moveIn(entry.cardId)
          }}
          className="h-11 shrink-0 rounded-button border border-border bg-surface-1 px-2 text-12 font-semibold hover:bg-surface-2"
        >
          {t("decks.side.moveIn")}
        </button>
      ) : (
        <CountStepper
          count={entry.count}
          max={Math.max(status.max, entry.count)}
          name={name.primary.text}
          onChange={(count) => {
            editor.setCount(section, entry.cardId, count)
          }}
        />
      )}
    </li>
  )
}

export function LeaderRow({
  images,
  onPickPrinting,
}: {
  readonly images: ImageIndex | undefined
  readonly onPickPrinting: (cardId: string) => void
}) {
  const { t } = useTranslation()
  const editor = useDeckEditor()
  const leader = editor.deck.leader
  const summary = leader ? editor.facts?.summaryOf(leader.cardId, leader.printingId) : undefined
  const facts = leader ? editor.facts?.factsOf(leader.cardId) : undefined
  const name = summary
    ? displayName(summary.name, editor.uiLanguage, editor.nameDisplay)
    : undefined
  const otherPrintings = (facts?.printings.length ?? 1) - 1
  return (
    <div
      data-deck-card={leader?.cardId}
      className={cn(
        "flex items-center gap-3 rounded-button border border-border bg-surface-1 p-2",
        leader !== null && editor.highlight === leader.cardId && "ring-2 ring-accent",
      )}
    >
      {summary && name ? (
        <CardImage
          summary={summary}
          name={name.primary}
          images={images}
          alt="decorative"
          size="large"
          className="w-14"
        />
      ) : (
        <span className="flex aspect-[63/88] w-14 shrink-0 items-center justify-center rounded-card border border-dashed border-border-strong text-text-3">
          <Crown className="size-5" aria-hidden="true" />
        </span>
      )}
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="text-11 font-semibold text-text-3">{t("decks.leader")}</span>
        {summary && name && leader ? (
          <>
            <button
              type="button"
              onClick={() => {
                editor.openCard(summary)
              }}
              className="truncate text-start text-14 font-semibold hover:underline"
              lang={name.primary.lang}
            >
              {name.primary.text}
            </button>
            <button
              type="button"
              onClick={() => {
                onPickPrinting(leader.cardId)
              }}
              aria-label={t("decks.pickPrintingNamed", { name: name.primary.text })}
              title={t("decks.pickPrinting")}
              className="inline-flex w-fit items-center gap-1 rounded-badge text-11 text-text-3 tabular-nums hover:text-text-1"
            >
              <Layers className="size-3" aria-hidden="true" />
              {summary.cardNo}
              {otherPrintings > 0 && <span>+{otherPrintings}</span>}
            </button>
          </>
        ) : (
          <span className="text-13 text-text-2">{t("decks.leaderMissingHint")}</span>
        )}
      </div>
      <button
        type="button"
        onClick={editor.pickLeader}
        className="h-11 shrink-0 rounded-button border border-border bg-surface-1 px-3 text-13 font-semibold hover:bg-surface-2"
      >
        {leader ? t("decks.changeLeader") : t("decks.chooseLeader")}
      </button>
    </div>
  )
}
