import { useEffect } from "react"
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
      <div
        className="fixed inset-0 z-50 overflow-y-auto bg-bg pb-21 lg:left-18"
        role="dialog"
        aria-modal="true"
      >
        <div className="mx-auto w-full max-w-320 px-4 lg:px-6">{content}</div>
      </div>
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
    </>
  )
}
