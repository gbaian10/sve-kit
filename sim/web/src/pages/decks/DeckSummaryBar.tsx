import { ChevronUp, CircleAlert, CircleCheck, CircleDashed } from "lucide-react"
import { useTranslation } from "react-i18next"

import { BottomBar } from "../../components/nav/BottomBar"
import { EVOLVE_MAX, MAIN_MAX, MAIN_MIN, totalCount } from "../../domain/deck/model"
import { useDeckEditor } from "./deck-editor"
import { CountPill } from "./DeckPanel"

/** The deck's check result: nothing is "legal" until the card data has loaded and the rules ran. */
function DeckStatus() {
  const { t } = useTranslation()
  const { facts, dataFailed, problems } = useDeckEditor()
  if (facts === null)
    return dataFailed ? (
      <CircleAlert
        className="size-5 shrink-0 text-danger"
        role="img"
        aria-label={t("results.loadFailed")}
      />
    ) : (
      <CircleDashed
        className="size-5 shrink-0 text-text-3"
        role="img"
        aria-label={t("results.dependenciesPending")}
      />
    )
  const errors = problems.filter((item) => item.severity === "error").length
  if (errors === 0)
    return (
      <CircleCheck
        className="size-5 shrink-0 text-success"
        role="img"
        aria-label={t("decks.status.legal")}
      />
    )
  return (
    <span
      className="inline-flex h-7 shrink-0 items-center gap-1 rounded-pill bg-danger-soft px-2 text-12 font-semibold text-danger"
      title={t("decks.status.problems", { count: errors })}
    >
      <CircleAlert className="size-4" aria-hidden="true" />
      <span className="sr-only">{t("decks.status.problems", { count: errors })}</span>
      <span aria-hidden="true">{errors}</span>
    </span>
  )
}

/**
 * Below the three-column width the deck's counts and problems take the site navigation's place:
 * a bar along the bottom, or on a landscape phone the navigation's 64px rail at the left.
 */
export function DeckSummaryBar({ onOpen }: { readonly onOpen: () => void }) {
  const { t } = useTranslation()
  const { deck } = useDeckEditor()
  return (
    <BottomBar className="phone-landscape:inset-y-0 phone-landscape:right-auto phone-landscape:w-16 phone-landscape:border-t-0 phone-landscape:border-r phone-landscape:pb-0">
      <div className="flex h-15.5 items-center gap-2 px-4 phone-landscape:h-full phone-landscape:flex-col phone-landscape:justify-center phone-landscape:gap-3 phone-landscape:px-1">
        <button
          type="button"
          onClick={onOpen}
          aria-label={t("decks.openDeckSheet")}
          className="flex min-h-11 min-w-0 flex-1 flex-col items-start justify-center gap-0.5 text-start phone-landscape:flex-none phone-landscape:items-center"
        >
          <span className="w-full truncate text-13 font-semibold text-text-1 phone-landscape:hidden">
            {deck.name}
          </span>
          <span className="flex items-center gap-1.5 phone-landscape:flex-col">
            <CountPill
              label={t("decks.pill.main")}
              count={totalCount(deck.main)}
              min={MAIN_MIN}
              max={MAIN_MAX}
              compactInRail
            />
            <CountPill
              label={t("decks.pill.evolve")}
              count={totalCount(deck.evolve)}
              min={0}
              max={EVOLVE_MAX}
              compactInRail
            />
          </span>
        </button>
        <DeckStatus />
        <button
          type="button"
          data-deck-open=""
          onClick={onOpen}
          className="inline-flex h-11 shrink-0 items-center gap-1 rounded-button bg-accent px-3 text-14 font-semibold text-accent-ink phone-landscape:px-1.5 phone-landscape:text-12"
        >
          {t("decks.deckSheet")}
          <ChevronUp className="size-4 phone-landscape:hidden" aria-hidden="true" />
        </button>
      </div>
    </BottomBar>
  )
}
