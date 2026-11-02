import { ChevronUp, CircleAlert, CircleCheck } from "lucide-react"
import { useTranslation } from "react-i18next"

import { BottomBar } from "../../components/nav/BottomBar"
import { EVOLVE_MAX, MAIN_MAX, MAIN_MIN, totalCount } from "../../domain/deck/model"
import { useDeckEditor } from "./deck-editor"
import { CountPill } from "./DeckPanel"

/** Below the three-column width the deck's counts and problems take the site navigation's place. */
export function DeckSummaryBar({ onOpen }: { readonly onOpen: () => void }) {
  const { t } = useTranslation()
  const { deck, problems } = useDeckEditor()
  const errors = problems.filter((item) => item.severity === "error").length
  return (
    <BottomBar>
      <div className="flex h-15.5 items-center gap-2 px-4">
        <button
          type="button"
          onClick={onOpen}
          aria-label={t("decks.openDeckSheet")}
          className="flex min-h-11 min-w-0 flex-1 flex-col items-start justify-center gap-0.5 text-start"
        >
          <span className="w-full truncate text-13 font-semibold text-text-1">{deck.name}</span>
          <span className="flex items-center gap-1.5">
            <CountPill
              label={t("decks.pill.main")}
              count={totalCount(deck.main)}
              min={MAIN_MIN}
              max={MAIN_MAX}
            />
            <CountPill
              label={t("decks.pill.evolve")}
              count={totalCount(deck.evolve)}
              min={0}
              max={EVOLVE_MAX}
            />
          </span>
        </button>
        {errors > 0 ? (
          <span
            className="inline-flex h-7 shrink-0 items-center gap-1 rounded-pill bg-danger-soft px-2 text-12 font-semibold text-danger"
            title={t("decks.status.problems", { count: errors })}
          >
            <CircleAlert className="size-4" aria-hidden="true" />
            <span className="sr-only">{t("decks.status.problems", { count: errors })}</span>
            <span aria-hidden="true">{errors}</span>
          </span>
        ) : (
          <CircleCheck
            className="size-5 shrink-0 text-success"
            aria-label={t("decks.status.legal")}
          />
        )}
        <button
          type="button"
          data-deck-open=""
          onClick={onOpen}
          className="inline-flex h-11 shrink-0 items-center gap-1 rounded-button bg-accent px-3 text-14 font-semibold text-accent-ink"
        >
          {t("decks.deckSheet")}
          <ChevronUp className="size-4" aria-hidden="true" />
        </button>
      </div>
    </BottomBar>
  )
}
