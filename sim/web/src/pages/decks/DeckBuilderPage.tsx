import { Crown } from "lucide-react"
import { type ReactNode, useCallback, useEffect, useMemo, useState } from "react"
import { useTranslation } from "react-i18next"
import { useParams, useSearchParams } from "react-router"

import { useDrawerSide, useSidebarLayout } from "../../app/cards-layout"
import { useDialogHistory } from "../../app/dialog-history"
import { useMediaQuery } from "../../app/useMediaQuery"
import { Button } from "../../components/ui/Button"
import { cn } from "../../components/ui/cn"
import { Dialog } from "../../components/ui/Dialog"
import { Link } from "../../components/ui/Link"
import { useToast } from "../../components/ui/toast-context"
import type { CardSummary } from "../../data"
import { SAME_NAME_LIMIT } from "../../domain/deck/model"
import { checkDeck } from "../../domain/deck/rules"
import type { Deck } from "../../domain/deck/types"
import { UI_TEXT_LANG } from "../../domain/nameDisplay"
import { currentUiLanguage } from "../../i18n"
import { decksStore, useDecks, usePrefs } from "../../settings"
import { PageTitle } from "../PageTitle"
import { CardPool, type PoolScope } from "./CardPool"
import { CountStepper } from "./CountStepper"
import { createDeckEditor, DeckEditorContext, useDeckEditor } from "./deck-editor"
import { DeckPanel } from "./DeckPanel"
import { DeckSummaryBar } from "./DeckSummaryBar"
import { useDeckData, useDeckImages } from "./useDeckData"

const HIGHLIGHT_MS = 1600

export function DeckBuilderPage() {
  const { deckId } = useParams()
  const { t } = useTranslation()
  const state = useDecks()
  if (state.status === "loading")
    return (
      <p role="status" className="py-10 text-center text-13 text-text-2">
        {t("decks.loading")}
      </p>
    )
  const deck = state.status === "ready" ? state.decks.find((item) => item.id === deckId) : undefined
  if (!deck)
    return (
      <div className="mx-auto flex max-w-lg flex-col items-center gap-3 py-10 text-center">
        <PageTitle>{t("decks.notFound")}</PageTitle>
        <p className="text-14 text-text-2">{t("decks.notFoundHint")}</p>
        <Link to="/decks">{t("decks.backToList")}</Link>
      </div>
    )
  return <DeckBuilder key={deck.id} deck={deck} />
}

/** The open card's control in its bottom bar: the stepper, or the leader button. */
function OverlayDeckAction({ summary }: { readonly summary: CardSummary }) {
  const { t } = useTranslation()
  const editor = useDeckEditor()
  const status = editor.statusOf(summary.cardId)
  const name = summary.name.original.text
  if (status.placement === "leader")
    return (
      <Button
        variant={status.isLeader ? "primary" : "secondary"}
        aria-pressed={status.isLeader}
        onClick={() => {
          editor.setLeader(status.isLeader ? null : summary.cardId)
        }}
      >
        <Crown className="size-4" aria-hidden="true" />
        {status.isLeader ? t("decks.cell.isLeader") : t("decks.cell.setLeader")}
      </Button>
    )
  if (status.placement === "none")
    return <span className="px-2 text-12 text-text-3">{t("decks.cell.notAllowed")}</span>
  return (
    <CountStepper
      count={status.count}
      max={Math.max(status.max, status.count)}
      limit={SAME_NAME_LIMIT}
      name={name}
      onChange={(count) => {
        if (count > status.count) editor.add(summary.cardId)
        else editor.remove(summary.cardId)
      }}
    />
  )
}

function DeckBuilder({ deck }: { readonly deck: Deck }) {
  const { t } = useTranslation()
  const toast = useToast()
  const prefs = usePrefs()
  const uiLanguage = currentUiLanguage(prefs.uiLanguage)
  const [params, setParams] = useSearchParams()
  // From the cards page's sidebar width (1180) the builder has three columns: deck, pool, filters.
  const threeColumns = useSidebarLayout()
  const veryWide = useMediaQuery("(min-width: 1440px)")
  const deckSide = useDrawerSide()
  const data = useDeckData(deck.edition)
  const { facts, restrictions, restrictionStatus } = data
  // The deck's own images come from the search worker: reprints have no image rows on the main thread.
  const images = useDeckImages(deck, facts, data.images)
  const [scope, setScope] = useState<PoolScope>("all")
  const [highlight, setHighlight] = useState<string | null>(null)
  // Below the three-column width the deck is a full-screen sheet over the pool; back closes it.
  const [sheetOpen, setSheetOpen] = useState(false)
  const closeSheet = useDialogHistory("deck-sheet", sheetOpen, () => {
    setSheetOpen(false)
  })
  // The pool hands over its card opener once it is ready; the deck list opens cards through it.
  const [opener, setOpener] = useState<{ readonly open: (summary: CardSummary) => void } | null>(
    null,
  )
  const onOpener = useCallback((open: ((summary: CardSummary) => void) | null) => {
    setOpener(open === null ? null : { open })
  }, [])
  const openCard = useCallback(
    (summary: CardSummary) => {
      opener?.open(summary)
    },
    [opener],
  )

  // The deck's edition decides which snapshot the page loads (`ed`, as on the cards page).
  useEffect(() => {
    if ((params.get("ed") === "en") === (deck.edition === "en")) return
    const next = new URLSearchParams(params)
    if (deck.edition === "en") next.set("ed", "en")
    else next.delete("ed")
    setParams(next, { replace: true })
  }, [deck.edition, params, setParams])

  useEffect(() => {
    if (highlight === null) return
    const timer = setTimeout(() => {
      setHighlight(null)
    }, HIGHLIGHT_MS)
    return () => {
      clearTimeout(timer)
    }
  }, [highlight])

  const problems = useMemo(
    () =>
      facts ? checkDeck(deck, { infoOf: (cardId) => facts.factsOf(cardId), restrictions }) : [],
    [deck, facts, restrictions],
  )
  const editor = useMemo(
    () =>
      createDeckEditor(
        {
          deck,
          facts,
          printingsReady: data.printingsReady,
          dataFailed: data.failed,
          retryData: data.retry,
          restrictions,
          problems,
          uiLanguage,
          textLang: UI_TEXT_LANG[uiLanguage],
          nameDisplay: prefs.nameDisplay,
          openCard,
          highlight,
          focusCard: (cardId) => {
            setHighlight(cardId)
            if (!threeColumns) setSheetOpen(true)
            requestAnimationFrame(() => {
              document
                .querySelector(`[data-deck-panel] [data-deck-card="${CSS.escape(cardId)}"]`)
                ?.scrollIntoView({ block: "center", behavior: "smooth" })
            })
          },
          pickLeader: () => {
            if (sheetOpen) closeSheet()
            setScope("leader")
            window.scrollTo({ top: 0, behavior: "smooth" })
          },
        },
        (next) => {
          decksStore.save(next).catch(() => {
            toast.show({ message: t("decks.saveFailed") })
          })
        },
      ),
    [
      deck,
      data.printingsReady,
      data.failed,
      data.retry,
      facts,
      restrictions,
      problems,
      uiLanguage,
      prefs.nameDisplay,
      openCard,
      highlight,
      threeColumns,
      sheetOpen,
      closeSheet,
      toast,
      t,
    ],
  )
  const deckAction = useCallback(
    (summary: CardSummary | undefined): ReactNode =>
      summary ? <OverlayDeckAction summary={summary} /> : null,
    [],
  )
  const panel = <DeckPanel images={images} restrictionStatus={restrictionStatus} />
  const pool = (
    <CardPool
      scope={scope}
      onScopeChange={setScope}
      onOpener={onOpener}
      deckAction={deckAction}
      // Below 1440 the filters start folded so the pool keeps four card columns.
      sidebarDefaultCollapsed={threeColumns && !veryWide}
    />
  )
  return (
    <DeckEditorContext value={editor}>
      <h1 className="sr-only">{deck.name}</h1>
      {threeColumns ? (
        // The deck takes the side the cards page keeps its card drawer on; the pool's own filter
        // column sits at the other edge. Deck column 340px, 380px from 1440.
        <div
          className={cn(
            "grid items-start gap-6 pb-6",
            deckSide === "left"
              ? "grid-cols-[340px_minmax(0,1fr)] xl:grid-cols-[380px_minmax(0,1fr)]"
              : "grid-cols-[minmax(0,1fr)_340px] xl:grid-cols-[minmax(0,1fr)_380px]",
          )}
        >
          <aside
            aria-label={t("decks.deckColumn")}
            className={cn(
              "sticky top-13 z-40 h-[calc(100dvh-3.25rem)] overflow-y-auto bg-bg py-3",
              deckSide === "left" ? "order-first -ml-3 pr-1 pl-3" : "order-last -mr-3 pr-3 pl-1",
            )}
          >
            {panel}
          </aside>
          {pool}
        </div>
      ) : (
        // The pool takes the page; the deck's summary bar replaces the site navigation.
        <div className="pb-24">
          {pool}
          <DeckSummaryBar
            onOpen={() => {
              setSheetOpen(true)
            }}
          />
          <Dialog
            open={sheetOpen}
            onClose={closeSheet}
            title={t("decks.deckSheet")}
            variant="card"
            footer={
              <Button variant="primary" size="lg" className="w-full" onClick={closeSheet}>
                {t("decks.continueAdding")}
              </Button>
            }
          >
            {panel}
          </Dialog>
        </div>
      )}
    </DeckEditorContext>
  )
}
