import { type KeyboardEvent, type SyntheticEvent, useEffect, useRef } from "react"
import { useTranslation } from "react-i18next"
import { useLocation, useNavigate, useParams } from "react-router"

import { useCardRoute } from "../../app/cardRoute"
import { neighbours, sequenceFor } from "../../app/cardSequence"
import { useCardView } from "../../app/cardView"
import type { CardEntryState } from "../../app/listEntryState"
import { useCatalog, useImageIndex } from "../../app/snapshot"
import { SkeletonGrid } from "../../components/results/SkeletonGrid"
import { cardPath } from "../../domain/route"
import { currentUiLanguage } from "../../i18n"
import { recentStore, usePrefs } from "../../settings"
import { CardsPage } from "../cards/CardsPage"
import { NotFoundPage } from "../not-found/NotFoundPage"
import { CardBottomBar } from "./CardBottomBar"
import { CardFirstScreen } from "./CardFirstScreen"

function isCardEntryState(value: unknown): value is CardEntryState {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as { background?: unknown }).background === "string" &&
    typeof (value as { resultKey?: unknown }).resultKey === "string"
  )
}

// Overlay only when the background list can be rebuilt from the entry state (architecture §2.2);
// a direct link, a refresh without state or a new tab gets the full page instead.
export function CardPage() {
  const { t } = useTranslation()
  const params = useParams()
  const location = useLocation()
  const navigate = useNavigate()
  const prefs = usePrefs()
  const uiLanguage = currentUiLanguage(prefs.uiLanguage)
  const { client, status, catalog } = useCatalog()
  const images = useImageIndex(client, catalog !== null)
  const route = useCardRoute(client, catalog, {
    ...(params["cardNo"] === undefined ? {} : { cardNo: params["cardNo"] }),
    ...(params["slug"] === undefined ? {} : { slug: params["slug"] }),
    ...(params["intId"] === undefined ? {} : { intId: params["intId"] }),
  })
  const printingId =
    route.status === "resolved" && route.kind === "found" ? route.printingId : undefined
  const card = useCardView(client, catalog, printingId, uiLanguage)
  const entry = isCardEntryState(location.state) ? location.state : undefined

  // Aliases and wrong slugs replace the URL with the canonical one, keeping the entry state.
  useEffect(() => {
    if (route.status === "resolved" && route.kind === "redirect")
      void navigate(route.to, { replace: true, state: location.state as unknown })
  }, [route, navigate, location.state])
  useEffect(() => {
    if (card.status === "ready") recentStore.push(card.view.printingId)
  }, [card])

  const summary = printingId === undefined ? undefined : catalog?.summary(printingId)
  const view = card.status === "ready" ? card.view : undefined
  const sequence = entry && catalog ? sequenceFor(entry, catalog, prefs.cardEdition) : []
  const around = entry && view ? neighbours(sequence, entry.resultKey, view.cardId) : undefined
  const go = (target: { readonly key: string; readonly printingId: string } | undefined) => {
    if (!target || !entry || !catalog) return
    const next = catalog.summary(target.printingId)
    if (!next) return
    void navigate(cardPath(next.cardNo, next.name.original.text), {
      replace: true,
      state: { ...entry, resultKey: target.key },
    })
  }
  const onEdition = (region: "jp" | "en") => {
    const target = view?.editions[region]
    const next = target === undefined ? undefined : catalog?.summary(target)
    if (!next) return
    void navigate(cardPath(next.cardNo, next.name.original.text), {
      replace: true,
      state: location.state as unknown,
    })
  }

  let content: React.ReactNode
  if (status.state === "error" || route.status === "failed" || card.status === "failed") {
    content = (
      <p role="alert" className="py-6 text-15 text-danger">
        {t("cardPage.loadFailed")}
      </p>
    )
  } else if (
    (route.status === "resolved" && route.kind === "missing") ||
    card.status === "missing"
  ) {
    content = (
      <NotFoundPage
        hint={t("cardPage.notFound", { cardNo: params["cardNo"] ?? params["intId"] ?? "" })}
      />
    )
  } else if (!view || !summary) {
    content = (
      <div className="pt-4">
        <p className="text-13 text-text-3" aria-live="polite">
          {t("cardPage.loading")}
        </p>
        <SkeletonGrid count={2} />
      </div>
    )
  } else {
    content = (
      <CardFirstScreen
        view={view}
        summary={summary}
        images={images}
        uiLanguage={uiLanguage}
        onBack={entry ? () => void navigate(-1) : undefined}
        onEdition={onEdition}
      />
    )
  }

  if (!entry) return <div className="pb-6">{content}</div>
  return (
    <>
      <CardsPage search={entry.background} pages={entry.pages} inert />
      <CardOverlay
        onClose={() => void navigate(-1)}
        ready={view !== undefined}
        bar={
          <CardBottomBar
            onPrev={
              around?.prev
                ? () => {
                    go(around.prev)
                  }
                : undefined
            }
            onNext={
              around?.next
                ? () => {
                    go(around.next)
                  }
                : undefined
            }
          />
        }
      >
        {content}
      </CardOverlay>
    </>
  )
}

// A native modal dialog: the browser makes everything else inert, traps focus and returns it to
// the opener when it closes (design §11). Escape and the back button both go back in history.
function CardOverlay({
  children,
  bar,
  onClose,
  ready,
}: {
  readonly children: React.ReactNode
  readonly bar: React.ReactNode
  readonly onClose: () => void
  /** The card has rendered: the back button exists and takes focus once. */
  readonly ready: boolean
}) {
  const ref = useRef<HTMLDialogElement>(null)
  useEffect(() => {
    const dialog = ref.current
    if (!dialog) return
    if (!dialog.open) dialog.showModal()
    return () => {
      if (dialog.open) dialog.close()
    }
  }, [])
  useEffect(() => {
    const dialog = ref.current
    if (!dialog || !ready || dialog.contains(document.activeElement)) return
    // showModal focuses the dialog itself; once the card is there, its first control (back) takes it.
    dialog.querySelector<HTMLElement>("button, a[href]")?.focus()
  }, [ready])
  const onCancel = (event: SyntheticEvent<HTMLDialogElement>) => {
    event.preventDefault()
    onClose()
  }
  // Escape is handled here and its default (the dialog's cancel request) suppressed, so one press
  // goes back exactly once; `cancel` still covers other close requests.
  const onKeyDown = (event: KeyboardEvent<HTMLDialogElement>) => {
    if (event.key !== "Escape") return
    event.preventDefault()
    onClose()
  }
  return (
    // eslint-disable-next-line jsx-a11y/no-noninteractive-element-interactions -- Escape closes the layer like a native cancel would
    <dialog
      ref={ref}
      onCancel={onCancel}
      onKeyDown={onKeyDown}
      className="fixed inset-0 m-0 h-dvh max-h-none w-full max-w-none overflow-y-auto bg-bg p-0 pb-21 text-text-1 lg:left-18 lg:w-[calc(100%-4.5rem)]"
    >
      <div className="mx-auto w-full max-w-320 px-4 lg:px-6">{children}</div>
      {bar}
    </dialog>
  )
}
