import { ArrowLeft, ChevronLeft, ChevronRight, Link2, Maximize2, X } from "lucide-react"
import { useCallback, useEffect, useEffectEvent, useMemo, useRef, useState } from "react"
import { useTranslation } from "react-i18next"
import { Link as RouterLink, type To, useLocation, useNavigate, useParams } from "react-router"

import type { PrefetchTarget } from "../../app/card-prefetch"
import { useDialogHistory } from "../../app/dialog-history"
import { type CardEntryState, routeState } from "../../app/listEntryState"
import { useOtherEdition } from "../../app/other-edition"
import {
  type SearchReadiness,
  useImageIndex,
  useRouteEdition,
  useSearchReadiness,
} from "../../app/snapshot"
import { StatusStrip } from "../../app/StatusStrip"
import { CardImage } from "../../components/card/CardImage"
import { CardNameHeading, PlainNameHeading } from "../../components/card/CardNameHeading"
import { CardTypeBadge } from "../../components/card/CardTypeBadge"
import { CLASS_ICON } from "../../components/card/classIcons"
import { DeckLocationBadges } from "../../components/card/DeckLocationBadges"
import { RarityBadges } from "../../components/card/RarityBadges"
import { StatCells } from "../../components/card/StatCells"
import { type VocabularyKind, vocabularyLabel } from "../../components/card/vocabulary"
import { Badge } from "../../components/ui/Badge"
import { Button } from "../../components/ui/Button"
import { cn } from "../../components/ui/cn"
import { Dialog } from "../../components/ui/Dialog"
import { Link } from "../../components/ui/Link"
import { useToast } from "../../components/ui/toast-context"
import {
  arrayValue,
  artCredits,
  cardPrintings,
  type CardRouteInput,
  type CardSummary,
  type Catalog,
  type FaceArtist,
  faceBasics,
  type ImageIndex,
  type LocatedCard,
  nameOf,
  type Row,
  type SnapshotClient,
  type SnapshotStatus,
  stringValue,
} from "../../data"
import { CardRouteError, printingPath } from "../../domain/cardRoute"
import { classStyle } from "../../domain/classes"
import { deckLocationBadges } from "../../domain/deckLocation"
import { displayName, UI_TEXT_LANG } from "../../domain/nameDisplay"
import { editionUrl, querySearch } from "../../domain/query/codec"
import { DEFAULT_QUERY } from "../../domain/query/model"
import type { Region } from "../../domain/search"
import { currentUiLanguage } from "../../i18n"
import { showsOtherEdition, usePrefs } from "../../settings"
import { PageTitle } from "../PageTitle"
import { frontArtOf, useCardSequence } from "./card-sequence"
import { CardQa } from "./CardQa"
import { CardRestrictions } from "./CardRestrictions"
import { CardTextPanel } from "./CardTextPanel"
import { CardWording } from "./CardWording"
import { printingGroups } from "./printing-order"
import { type CardLink, type PrintingEntry, RelatedCards } from "./RelatedCards"
import { routeInput, useCardContent } from "./useCardContent"

const ICON_BUTTON =
  "flex size-11 shrink-0 items-center justify-center rounded-button text-text-1 hover:bg-surface-2"
// Phone: 72% of the width, as the reference sites do; tablet and desktop: the 340px column.
const IMAGE_SLOT = "w-[72%] max-w-85 lg:w-full lg:max-w-none"
const IMAGE_SIZES = "(min-width: 600px) 340px, 72vw"

export function CardPage() {
  const { cardNo, intId } = useParams()
  const edition = useRouteEdition()
  const location = useLocation()
  const routeKey = JSON.stringify((location.state as { route?: unknown } | null)?.route ?? null)
  const input = useMemo<CardRouteInput>(
    () =>
      routeInput(
        cardNo === undefined ? "provisional" : "official",
        cardNo ?? intId ?? "",
        edition,
        (JSON.parse(routeKey) as CardEntryState["route"] | null) ?? undefined,
      ),
    [routeKey, cardNo, intId, edition],
  )
  const content = useCardContent(input)
  return <CardView {...content} />
}

/** A printing another link in the overlay points at, with what the link already knows about it. */
export interface OverlayLinkTarget {
  readonly cardNo: string
  readonly cardNoState?: string
  readonly intId?: number
  readonly route?: CardEntryState["route"]
  readonly resultKey?: string
}

/** The list's panel or modal around the card: same content as the page, other links and chrome. */
export interface CardFrame {
  /** What the list knew when it opened the card: background, pages, resultKey, route. */
  readonly entry: Record<string, unknown>
  /** The route key shown until the card resolves. */
  readonly key: string
  /** Where a link to another printing goes while the overlay stays open. */
  readonly link: (target: OverlayLinkTarget) => { readonly to: To; readonly state: unknown }
  /** The card was found in the list under another key (its card or art cell). */
  readonly onPlaced: (resultKey: string) => void
  readonly onClose: () => void
}

export interface CardViewProps {
  readonly client: SnapshotClient
  readonly status: SnapshotStatus
  readonly catalog: Catalog | null
  readonly resolved: LocatedCard["printing"] | undefined
  readonly onRetry?: () => void
  readonly routeError?: unknown
  readonly pending?: boolean
  readonly showPending?: boolean
  readonly onPrefetch?: (targets: readonly PrefetchTarget[], signal: AbortSignal) => Promise<void>
  readonly preparedImages?: ImageIndex | undefined
  readonly frame?: CardFrame
}

export function CardView({
  client,
  status,
  catalog,
  resolved,
  routeError,
  onRetry,
  pending = false,
  showPending = pending,
  preparedImages,
  onPrefetch,
  frame,
}: CardViewProps) {
  const { t } = useTranslation()
  const { cardNo, intId } = useParams()
  const location = useLocation()
  const navigate = useNavigate()
  const observedImages = useImageIndex(client, catalog !== null && preparedImages === undefined)
  const images = preparedImages ?? observedImages
  const prefs = usePrefs()
  const cardEdition = resolved?.region ?? prefs.cardEdition
  const searchReadiness = useSearchReadiness(cardEdition)
  const failure =
    status.state === "error" ? status : status.state === "ready" ? status.updateError : undefined
  const { error: _searchError, loading: _searchLoading, ...searchState } = searchReadiness
  const readiness: SearchReadiness = {
    ...searchState,
    phase: status.state === "error" ? "error" : catalog ? "ready" : "downloading",
    updating: status.state === "ready" && !!status.updating,
    ...(failure ? { error: { kind: "load-failed" as const, message: failure.detail } } : {}),
    ...(status.state === "loading"
      ? {
          loading: {
            root: searchReadiness.root,
            edition: cardEdition,
            step: status.phase,
            ...(status.total === undefined
              ? {}
              : { total: status.total, done: status.done ?? 0, persistent: true }),
          },
        }
      : {}),
    retry: () => {
      void client.retry()
    },
  }
  const toast = useToast()
  const [zoomed, setZoomed] = useState<string | null>(null)
  const closeZoom = useDialogHistory("card-zoom", zoomed !== null, () => {
    setZoomed(null)
  })
  const uiLanguage = currentUiLanguage(prefs.uiLanguage)
  const textLang = UI_TEXT_LANG[uiLanguage]
  const printing = resolved ? catalog?.index.printing(resolved.id) : undefined
  const printingId = printing ? stringValue(printing["id"]) : null
  const artId = frontArtOf(printing)
  const region = (printing?.["region"] as Region | undefined) ?? cardEdition
  const faces = printing ? (catalog?.index.facesOf(printing["card_id"] as string) ?? []) : []
  const summary: CardSummary | undefined = printingId ? catalog?.summary(printingId) : undefined
  const card = printing ? catalog?.index.card(stringValue(printing["card_id"])) : undefined
  const family = card ? catalog?.index.family(stringValue(card["home_set_id"])) : undefined
  const setCode = family ? stringValue(family["public_code"]) : undefined
  const showOther = showsOtherEdition(prefs, region)
  const other = useOtherEdition(
    client,
    region,
    showOther,
    catalog !== null && searchReadiness.phase === "ready" && !searchReadiness.updating,
    client.snapshot()?.manifestHash,
  )
  const cardId = typeof card?.["id"] === "string" ? card["id"] : undefined
  const [printingAttempt, setPrintingAttempt] = useState(0)
  const [printingCatalog, setPrintingCatalog] = useState<{
    source: Catalog
    cardId: string
    other: Catalog | undefined
    value: Catalog
    error?: unknown
  }>()
  useEffect(() => {
    const snapshot = other.snapshot ?? client.snapshot()
    if (!snapshot || !catalog || !cardId) return
    let active = true
    void cardPrintings(client, snapshot, cardId, other.snapshot ? ["jp", "en"] : [region]).then(
      (value) => {
        if (active) setPrintingCatalog({ source: catalog, cardId, other: other.catalog, value })
      },
      (error: unknown) => {
        if (active)
          setPrintingCatalog({
            source: catalog,
            cardId,
            other: other.catalog,
            value: catalog,
            error,
          })
      },
    )
    return () => {
      active = false
    }
  }, [client, catalog, cardId, region, other.snapshot, other.catalog, printingAttempt])
  // Artists come from the art rows of the card's home set; they appear once those files are in.
  const owner = card?.["home_set_id"]
  const faceArts = arrayValue(printing?.["faces"] ?? []).map((raw) => {
    const face = raw as Row
    return [stringValue(face["face_id"]), stringValue(face["art_id"] ?? "")] as const
  })
  const artsKey = JSON.stringify([client.base, client.snapshot()?.manifestHash, region, faceArts])
  // `value` is absent when the read failed: then the line stays out instead of claiming no credit.
  const [credited, setCredited] = useState<{
    key: string
    value?: ReadonlyMap<string, readonly FaceArtist[]>
  }>()
  useEffect(() => {
    const snapshot = client.snapshot()
    const artIds = faceArts.map(([, art]) => art).filter((art) => art !== "")
    if (!snapshot || typeof owner !== "string" || artIds.length === 0) return
    let active = true
    void artCredits(client, snapshot, owner, region, artIds).then(
      (value) => {
        if (active) setCredited({ key: artsKey, value })
      },
      () => {
        // The artist line is optional; a failed read leaves it out instead of blocking the card.
        if (active) setCredited({ key: artsKey })
      },
    )
    return () => {
      active = false
    }
    // faceArts is derived from artsKey
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [client, owner, region, artsKey])
  const completePrintings =
    printingCatalog?.source === catalog &&
    printingCatalog.cardId === cardId &&
    printingCatalog.other === other.catalog
      ? printingCatalog
      : undefined
  const printings = cardId
    ? ((completePrintings?.value ?? catalog)?.index.printingsOf(cardId) ?? []).filter(
        (row) => row["region"] === region || (showOther && other.catalog),
      )
    : []
  // Opened from a list: back pops to it and previous/next walk that list; opened directly: back
  // is a link to the search page and previous/next walk the snapshot order.
  const entry =
    frame?.entry ??
    (typeof location.state === "object" && location.state !== null
      ? (location.state as Record<string, unknown>)
      : null)
  const fromList = typeof entry?.["background"] === "string"
  // While switching, the shown printing is still the old one: it must not place the new card.
  const sequence = useCardSequence(entry, cardEdition, pending ? null : printingId, cardId, artId)
  const previous = sequence.phase === "found" ? sequence.previous : undefined
  const next = sequence.phase === "found" ? sequence.next : undefined
  // A card placed through its card or art cell tells the list which cell to mark. Only the list's
  // own state changes: writing history here could land on an entry the user already left.
  const placedKey = sequence.phase === "found" ? sequence.key : undefined
  const storedKey = typeof entry?.["resultKey"] === "string" ? entry["resultKey"] : undefined
  const place = useEffectEvent((key: string) => {
    frame?.onPlaced(key)
  })
  useEffect(() => {
    if (placedKey !== undefined && placedKey !== storedKey) place(placedKey)
  }, [placedKey, storedKey])
  const [readyImage, setReadyImage] = useState<string | null>(null)
  const imageKey = JSON.stringify([client.base, client.snapshot()?.manifestHash, printingId])
  const prefetchAbort = useRef<AbortController | null>(null)
  const navigationFrom = useRef<string | null>(null)
  const beginNavigation = useCallback(() => {
    navigationFrom.current = imageKey
    prefetchAbort.current?.abort()
  }, [imageKey])
  // A transition can render the old page again before committing its new target.
  useEffect(() => {
    if (!pending && navigationFrom.current !== imageKey) {
      navigationFrom.current = null
    }
  }, [pending, imageKey])
  const imageReady = useCallback(() => {
    setReadyImage(imageKey)
  }, [imageKey])
  const previousSummary = previous?.summary
  const nextSummary = next?.summary
  useEffect(() => {
    if (pending || navigationFrom.current === imageKey || readyImage !== imageKey || !onPrefetch)
      return
    const targets: PrefetchTarget[] = []
    if (prefs.preloadNeighbours) {
      for (const summary of [previousSummary, nextSummary])
        if (summary) targets.push({ summary, neighbour: true })
    }
    if (prefs.preloadPrintings && completePrintings && !completePrintings.error) {
      for (const row of completePrintings.value.index.printingsOf(cardId ?? "")) {
        if (row["region"] !== region && !(showOther && other.catalog)) continue
        const summary = completePrintings.value.summary(stringValue(row["id"]))
        if (summary && summary.printingId !== printingId)
          targets.push({ summary, neighbour: false })
      }
    }
    const abort = new AbortController()
    prefetchAbort.current = abort
    void onPrefetch(targets, abort.signal)
    return () => {
      abort.abort()
    }
  }, [
    pending,
    readyImage,
    imageKey,
    onPrefetch,
    prefs.preloadNeighbours,
    prefs.preloadPrintings,
    previousSummary,
    nextSummary,
    completePrintings,
    cardId,
    region,
    showOther,
    other.catalog,
    printingId,
  ])
  const canonical =
    summary && !frame
      ? summary.cardNoState === "provisional"
        ? printingPath(summary)
        : `${printingPath(summary)}/${encodeURIComponent(summary.name.original.text)}`
      : null
  useEffect(() => {
    if (!pending && canonical && location.pathname !== canonical)
      void navigate(canonical, { replace: true, state: location.state as unknown })
  }, [pending, canonical, summary?.cardNoState, location.pathname, location.state, navigate])
  const open = (target: NonNullable<typeof previous>) => {
    if (target.summary.printingId !== printingId) beginNavigation()
    if (frame) {
      const { cardNo, cardNoState, intId } = target.summary
      const link = frame.link({
        cardNo,
        ...(cardNoState === undefined ? {} : { cardNoState }),
        ...(intId === undefined ? {} : { intId }),
        ...routeState(target.summary),
        resultKey: target.key,
      })
      void navigate(link.to, { replace: true, state: link.state, preventScrollReset: true })
      return
    }
    void navigate(printingPath(target.summary), {
      replace: true,
      state: { ...entry, resultKey: target.key, ...routeState(target.summary) },
    })
  }
  // The page URL the panel's copy button hands out; the page itself copies its own address.
  const pageUrl = summary
    ? `${printingPath(summary)}${summary.region === "en" ? "?ed=en" : ""}`
    : null
  const openFullPage = () => {
    if (!summary) return
    const { overlay: _overlay, ...listEntry } = entry ?? {}
    void navigate(pageUrl ?? printingPath(summary), {
      state: { ...listEntry, ...routeState(summary) },
    })
  }
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey) return
      const target = event.target
      // The card modal is itself a dialog; only other dialogs (zoom, filters) keep the arrows.
      if (
        target instanceof Element &&
        (target.closest("input, textarea, select, [contenteditable=true]") !== null ||
          (target.closest("dialog") !== null && target.closest("[data-card-overlay]") === null))
      )
        return
      if (event.key === "ArrowLeft" && previous) open(previous)
      else if (event.key === "ArrowRight" && next) open(next)
      else return
      event.preventDefault()
    }
    const onLink = (event: MouseEvent) => {
      const link = event.target instanceof Element ? event.target.closest("a[href]") : null
      if (
        !(link instanceof HTMLAnchorElement) ||
        link.target === "_blank" ||
        event.button !== 0 ||
        event.metaKey ||
        event.ctrlKey ||
        event.shiftKey ||
        event.altKey
      )
        return
      const target = new URL(link.href)
      if (summary && target.pathname === printingPath(summary) && target.search === location.search)
        return
      if (
        target.origin === window.location.origin &&
        (target.pathname !== location.pathname || target.search !== location.search)
      )
        beginNavigation()
    }
    document.addEventListener("click", onLink, true)
    const onPopState = () => {
      if (
        window.location.pathname !== location.pathname ||
        window.location.search !== location.search
      )
        beginNavigation()
    }
    window.addEventListener("popstate", onPopState)
    window.addEventListener("keydown", onKeyDown)
    return () => {
      document.removeEventListener("click", onLink, true)
      window.removeEventListener("popstate", onPopState)
      window.removeEventListener("keydown", onKeyDown)
    }
  })
  const copyLink = async () => {
    try {
      await navigator.clipboard.writeText(
        frame && pageUrl ? new URL(pageUrl, window.location.href).href : window.location.href,
      )
      toast.show({ message: t("cardPage.linkCopied") })
    } catch {
      toast.show({ message: t("cardPage.linkCopyFailed") })
    }
  }
  const index = catalog?.index
  const label = (kind: VocabularyKind, code: string) =>
    index ? vocabularyLabel(index, kind, code, textLang) : code
  const joiner = t("cardPage.separator")
  // Where a link to a card goes: inside the overlay when there is one, else to its page. Another
  // printing of this card keeps the list entry (back, previous/next); another card starts afresh.
  const cardLink = (
    target: {
      readonly cardNo: string
      readonly cardNoState?: string
      readonly intId?: number
      readonly route?: CardEntryState["route"]
    },
    sameCard: boolean,
  ): CardLink => {
    const { cardNo, cardNoState, intId, route } = target
    if (frame)
      return frame.link({
        cardNo,
        ...(cardNoState === undefined ? {} : { cardNoState }),
        ...(intId === undefined ? {} : { intId }),
        ...(route === undefined ? {} : { route }),
      })
    const { resultKey: _resultKey, ...listEntry } = entry ?? {}
    return {
      to: printingPath(target),
      state: { ...(sameCard ? entry : listEntry), ...(route === undefined ? {} : { route }) },
    }
  }
  const printingEntries = printingGroups(printings).map(
    ({ region: printingRegion, printings: regionalPrintings }) => ({
      region: printingRegion,
      printings: regionalPrintings.map((row) => {
        const id = stringValue(row["id"])
        const rarity =
          typeof row["rarity_code"] === "string"
            ? label("rarity", row["rarity_code"])
            : typeof row["rarity_raw"] === "string"
              ? row["rarity_raw"]
              : undefined
        const link = cardLink(
          {
            cardNo: stringValue(row["card_no"]),
            ...(typeof row["card_no_state"] === "string"
              ? { cardNoState: row["card_no_state"] }
              : {}),
            ...(typeof row["int_id"] === "number" ? { intId: row["int_id"] } : {}),
            route: {
              cardId: stringValue(row["card_id"]),
              printingId: id,
              cardOwner: stringValue(card?.["home_set_id"]),
              printingOwner: stringValue(row["home_set_id"]),
              region: printingRegion,
            },
          },
          true,
        )
        return { id, cardNo: stringValue(row["card_no"]), rarity, link }
      }),
    }),
  )
  const printingLinks = new Map(
    printingEntries.flatMap(({ printings: rows }) => rows.map(({ id, link }) => [id, link])),
  )
  // Thumbnails need the printing's summary, which arrives with the printing metadata.
  const printingThumbnails: PrintingEntry[] = printingEntries.flatMap(({ printings: rows }) =>
    rows.flatMap(({ id, rarity, link }) => {
      const summary = (completePrintings?.value ?? catalog)?.summary(id)
      return summary ? [{ summary, rarity, link }] : []
    }),
  )

  if (routeError || (catalog !== null && !printing)) {
    const kind = routeError instanceof CardRouteError ? routeError.kind : undefined
    // A key the route contract rejects names no card, the same as an unknown number.
    const missing = kind === "not-found" || kind === "invalid"
    const title =
      kind === "needs-update"
        ? t("cardPage.needsUpdate")
        : missing
          ? t("cardPage.notFound")
          : t("cardPage.routeFailed")
    return (
      <>
        {frame ? (
          <div className="flex h-13 items-center justify-between gap-1">
            <h2 className="text-16 font-semibold">{title}</h2>
            <button
              type="button"
              aria-label={t("cardPage.closePanel")}
              title={t("cardPage.closePanel")}
              onClick={frame.onClose}
              className={ICON_BUTTON}
            >
              <X className="size-5" aria-hidden="true" />
            </button>
          </div>
        ) : (
          <PageTitle>{title}</PageTitle>
        )}
        <p role="alert" className="text-15 text-text-2">
          {missing ? t("cardPage.notFoundHint") : t("cardPage.routeFailedHint")}
        </p>
        {onRetry && (
          <Button className="mt-4" onClick={onRetry}>
            {t("status.retry")}
          </Button>
        )}
        <p className="mt-4">
          <Link to="/cards">{t("nav.cards")}</Link>
        </p>
      </>
    )
  }
  const faceCatalog = catalog
  const views = faces.map((face, ordinal) => {
    const faceId = face["id"] as string
    const wording = faceCatalog?.index.wording(faceId, region)
    const basis = faceCatalog?.index.displayRevision(faceId, region)
    const revision = faceCatalog?.index.displayRevision(faceId, region, printingId ?? undefined)
    const nameRow = revision ?? (wording?.["common_name"] as Row | null | undefined)
    const backName =
      ordinal > 0 && nameRow && faceCatalog
        ? nameOf(faceCatalog.index, nameRow, "name", basis ?? nameRow)
        : undefined
    const faceSummary: CardSummary | undefined =
      summary &&
      (ordinal === 0
        ? summary
        : {
            ...summary,
            faceId,
            name: backName ?? {
              original: { lang: region === "jp" ? "ja" : "en", text: summary.cardNo },
              translations: {},
            },
            wordingPending: wording !== undefined && wording["classification"] !== "same_effect",
          })
    const name = faceSummary
      ? displayName(faceSummary.name, uiLanguage, prefs.nameDisplay)
      : undefined
    return { faceId, ordinal, wording, revision, faceSummary, name }
  })
  const zoomedView = views.find((view) => view.faceId === zoomed)
  const Heading = frame ? "h2" : "h1"
  const navigation = (sequence.phase === "pending" || sequence.phase === "found") && (
    <>
      <Button
        data-card-navigation
        className={cn("h-10 gap-1 px-3 text-14", frame && "flex-1")}
        disabled={previous === undefined}
        onClick={() => {
          if (previous) open(previous)
        }}
      >
        <ChevronLeft className="size-4.5" aria-hidden="true" />
        {t("cardPage.previous")}
      </Button>
      {/* Where "add to deck" will sit; there is no deck builder yet, so no control is rendered. */}
      {frame && <span aria-hidden="true" data-deck-slot="" className="w-14 shrink-0" />}
      <Button
        data-card-navigation
        className={cn("h-10 gap-1 px-3 text-14", frame && "flex-1")}
        disabled={next === undefined}
        onClick={() => {
          if (next) open(next)
        }}
      >
        {t("cardPage.next")}
        <ChevronRight className="size-4.5" aria-hidden="true" />
      </Button>
    </>
  )
  return (
    <article
      aria-busy={pending}
      className={cn("relative w-full", frame ? "flex min-h-full flex-col" : "mx-auto max-w-270")}
    >
      {pending && showPending && catalog && (
        <p
          role="status"
          className={cn(
            "pointer-events-none absolute top-13 z-10 rounded-control bg-surface-2 px-3 py-1 text-13 text-text-2 shadow",
            frame ? "right-4" : "right-0",
          )}
        >
          {t("cardPage.switchingCard")}
        </p>
      )}
      {!frame && <StatusStrip readiness={readiness} />}
      {/* Top row (design 06): back 44, set tag 28, number centred in Plex 600 15px, copy link 44. */}
      <div
        className={cn(
          "flex h-13 items-center gap-1",
          frame && "sticky top-0 z-20 bg-surface-1 px-4",
        )}
      >
        {frame ? null : fromList ? (
          <button
            type="button"
            aria-label={t("cardPage.back")}
            title={t("cardPage.back")}
            onClick={() => void navigate(-1)}
            className={ICON_BUTTON}
          >
            <ArrowLeft className="size-5.5" aria-hidden="true" />
          </button>
        ) : (
          <RouterLink
            to={region === "en" ? "/cards?ed=en" : "/cards"}
            aria-label={t("cardPage.back")}
            title={t("cardPage.back")}
            className={ICON_BUTTON}
          >
            <ArrowLeft className="size-5.5" aria-hidden="true" />
          </RouterLink>
        )}
        {setCode !== undefined && (
          <RouterLink
            to={`/sets/${encodeURIComponent(setCode)}${region === "en" ? "?ed=en" : ""}`}
            aria-label={t("cardPage.set", { code: setCode })}
            className="flex h-7 shrink-0 items-center rounded-sm bg-surface-2 px-2 text-12 font-semibold text-text-2 hover:bg-surface-3"
          >
            {setCode}
          </RouterLink>
        )}
        <Heading className="min-w-0 flex-1 truncate text-center font-sans text-15 font-semibold tabular-nums">
          {resolved?.cardNo ?? frame?.key ?? cardNo ?? intId}
        </Heading>
        <button
          type="button"
          aria-label={t("cardPage.copyLink")}
          title={t("cardPage.copyLink")}
          onClick={() => void copyLink()}
          className={ICON_BUTTON}
        >
          <Link2 className="size-5" aria-hidden="true" />
        </button>
        {frame && (
          <>
            <button
              type="button"
              aria-label={t("cardPage.fullscreen")}
              title={t("cardPage.fullscreen")}
              disabled={!summary}
              onClick={openFullPage}
              className={ICON_BUTTON}
            >
              <Maximize2 className="size-5" aria-hidden="true" />
            </button>
            <button
              type="button"
              aria-label={t("cardPage.closePanel")}
              title={t("cardPage.closePanel")}
              onClick={frame.onClose}
              className={ICON_BUTTON}
            >
              <X className="size-5" aria-hidden="true" />
            </button>
          </>
        )}
      </div>
      {/* Phone and tablet: the large image first, then the text; desktop: a sticky image column. */}
      <div
        className={cn(
          frame === undefined && "lg:grid lg:grid-cols-[340px_minmax(0,1fr)] lg:gap-x-10",
          // The drawer and the modal are containers: side by side only once they are wide enough.
          frame !== undefined && "px-4 @4xl:grid @4xl:grid-cols-[280px_minmax(0,1fr)] @4xl:gap-x-6",
        )}
        onClickCapture={(event) => {
          if (
            pending &&
            event.target instanceof Element &&
            !event.target.closest("[data-card-navigation]")
          ) {
            event.preventDefault()
            event.stopPropagation()
          }
        }}
      >
        <div
          className={cn(
            "mt-1 flex flex-col items-center gap-3",
            frame === undefined && "lg:sticky lg:top-4 lg:self-start",
            frame !== undefined && "@4xl:sticky @4xl:top-13 @4xl:self-start",
          )}
        >
          {views.map(
            ({ faceId, ordinal, faceSummary, name }) =>
              faceSummary &&
              name && (
                <button
                  key={faceId}
                  type="button"
                  disabled={pending}
                  aria-label={t("cardPage.zoom", { name: name.primary.text })}
                  onClick={() => {
                    setZoomed(faceId)
                  }}
                  className={cn(
                    "block rounded-card shadow-lg transition-transform hover:scale-[1.01]",
                    frame === undefined && IMAGE_SLOT,
                    frame !== undefined && "w-[64%] max-w-64 @4xl:w-full @4xl:max-w-none",
                  )}
                >
                  <CardImage
                    summary={faceSummary}
                    onReady={ordinal === 0 ? imageReady : undefined}
                    name={name.primary}
                    images={images}
                    alt="decorative"
                    sizes={IMAGE_SIZES}
                    className="w-full"
                  />
                </button>
              ),
          )}
        </div>
        <div className="mt-4 min-w-0 lg:mt-1">
          {cardId && catalog && (
            <CardRestrictions client={client} cardId={cardId} region={region} />
          )}
          {views.map(({ faceId, ordinal, wording, revision, faceSummary, name }) => {
            const basics =
              faceCatalog && printingId
                ? faceBasics(faceCatalog.index, faceId, region, printingId)
                : undefined
            const nameRevisionId = basics?.revision?.["id"]
            const values = basics?.values
            const classCode =
              typeof values?.["class_code"] === "string"
                ? values["class_code"]
                : values?.["class_code"] === null
                  ? null
                  : undefined
            const typeCode = typeof values?.["type_code"] === "string" ? values["type_code"] : null
            const locations = deckLocationBadges(
              typeCode ?? "",
              arrayValue(values?.["special_kinds"] ?? []),
            )
            const traits = (values?.["traits"] ?? []) as string[]
            // Undefined until the credits are read (or when the read failed): nothing is claimed then.
            const faceArt = faceArts.find(([face]) => face === faceId)?.[1] ?? ""
            const artistsOfFace =
              credited?.key === artsKey && credited.value !== undefined && faceArt !== ""
                ? (credited.value.get(faceArt) ?? [])
                : undefined
            const cost = typeof values?.["cost"] === "number" ? values["cost"] : null
            const attack = typeof values?.["attack"] === "number" ? values["attack"] : null
            const defense = typeof values?.["defense"] === "number" ? values["defense"] : null
            return (
              <section
                key={faceId}
                aria-label={name?.primary.text ?? faceId}
                className={cn(ordinal > 0 && "mt-6 border-t border-border pt-4")}
              >
                {faceSummary && name && (
                  <div className="flex flex-col gap-1.5">
                    {ordinal > 0 && <Badge className="self-start">{t("cardPage.backFace")}</Badge>}
                    <div className="flex items-start gap-2">
                      <div className="min-w-0 flex-1">
                        {typeof nameRevisionId === "string" ? (
                          <CardNameHeading
                            client={client}
                            edition={region}
                            resolver={null}
                            owner={{ kind: "face_revision", id: nameRevisionId }}
                            fallback={name}
                          />
                        ) : (
                          <PlainNameHeading name={name} />
                        )}
                      </div>
                    </div>
                    <p
                      data-card-classification=""
                      className="flex flex-wrap items-center gap-x-1.5 gap-y-1 text-13 text-text-2"
                    >
                      {classCode !== undefined && (
                        <>
                          <img
                            src={CLASS_ICON[classStyle(classCode)]}
                            alt=""
                            className="size-5 shrink-0"
                          />
                          <span>
                            {classCode === null ? t("search.neutral") : label("class", classCode)}
                            {joiner}
                          </span>
                        </>
                      )}
                      {typeCode !== "leader" && (
                        <CardTypeBadge
                          code={typeCode}
                          label={typeCode ? label("type", typeCode) : ""}
                          fallback
                        />
                      )}
                      <DeckLocationBadges locations={locations} />
                      <RarityBadges {...faceSummary} />
                      {traits.length > 0 && (
                        <span>{traits.map((code) => label("trait", code)).join(joiner)}</span>
                      )}
                    </p>
                    {artistsOfFace?.length === 0 && (
                      <p data-card-artists="none" className="text-13 text-text-3">
                        {t("cardPage.artist")}
                        {t("cardPage.artistNone")}
                      </p>
                    )}
                    {artistsOfFace !== undefined && artistsOfFace.length > 0 && (
                      <p data-card-artists="" className="text-13 text-text-2">
                        {t("cardPage.artist")}
                        {artistsOfFace.map((artist, index) => (
                          <span key={artist.id}>
                            {index > 0 && joiner}
                            <Link
                              to={editionUrl(
                                `/cards${querySearch({ ...DEFAULT_QUERY, artists: [artist.id] })}`,
                                region,
                              )}
                              aria-label={t("cardPage.artistLink", { name: artist.name })}
                            >
                              {artist.name}
                            </Link>
                          </span>
                        ))}
                      </p>
                    )}
                    <StatCells className="mt-1" cost={cost} attack={attack} defense={defense} />
                    {!frame && ordinal === 0 && navigation && (
                      <div className="mt-2 flex gap-2">{navigation}</div>
                    )}
                  </div>
                )}
                {printingId && catalog && (
                  <CardTextPanel
                    cardId={cardId ?? ""}
                    key={printingId}
                    client={client}
                    edition={region}
                    printingId={printingId}
                    faceId={faceId}
                  >
                    <CardWording
                      client={client}
                      edition={region}
                      // The completed printings carry rarity and premium, which order the chips
                      // and so the printing named by the wording note.
                      catalog={completePrintings?.value ?? catalog}
                      printingId={printingId}
                      wording={wording}
                      revision={revision}
                      linkOf={(id) => printingLinks.get(id)}
                    />
                  </CardTextPanel>
                )}
              </section>
            )
          })}
          {catalog && cardId !== undefined && (
            <CardQa
              key={`qa:${cardId}`}
              client={client}
              cardId={cardId}
              edition={region}
              showOther={showOther}
            />
          )}
          {showOther && !other.catalog && !other.error && (
            <p role="status" className="mt-6 text-13 text-text-2">
              {t("cardPage.switchingEdition")}
            </p>
          )}
          {(other.error !== undefined || completePrintings?.error !== undefined) && (
            <div role="alert" className="mt-6 text-13 text-text-2">
              {t("cardPage.otherEditionFailed")}
              <Button
                onClick={() => {
                  other.retry()
                  setPrintingAttempt((value) => value + 1)
                }}
              >
                {t("status.retry")}
              </Button>
            </div>
          )}
          {printings.length > 1 && (
            <section className="mt-6" aria-label={t("cardPage.printings")}>
              <h3 className="text-13 font-semibold text-text-3">{t("cardPage.printings")}</h3>
              {printingEntries.map(({ region: printingRegion, printings: regionalPrintings }) => (
                <div key={printingRegion} className="mt-1.5 flex items-start gap-2">
                  <span className="inline-flex h-8 items-center text-13 font-semibold text-text-3">
                    {printingRegion.toUpperCase()}:
                  </span>
                  <ul aria-label={printingRegion.toUpperCase()} className="flex flex-wrap gap-2">
                    {regionalPrintings.map(({ id, cardNo: no, rarity, link }) => (
                      <li key={id}>
                        <RouterLink
                          to={link.to}
                          state={link.state}
                          replace
                          preventScrollReset
                          aria-current={id === printingId ? "page" : undefined}
                          className="inline-flex h-8 items-center rounded-pill border border-border px-2.5 font-mono text-13 hover:bg-surface-2 aria-[current=page]:border-accent aria-[current=page]:bg-accent-soft aria-[current=page]:font-semibold"
                        >
                          {no}
                          {rarity !== undefined && rarity !== "" && ` (${rarity})`}
                        </RouterLink>
                      </li>
                    ))}
                  </ul>
                </div>
              ))}
            </section>
          )}
          {catalog && cardId !== undefined && printingId !== null && (
            <RelatedCards
              key={`related:${cardId}`}
              client={client}
              catalog={catalog}
              edition={region}
              cardId={cardId}
              printingId={printingId}
              revisionIds={views.flatMap(({ revision }) =>
                typeof revision?.["id"] === "string" ? [revision["id"]] : [],
              )}
              printings={printingThumbnails}
              otherSnapshot={showOther ? other.snapshot : undefined}
              linkTo={(target) =>
                cardLink(
                  {
                    cardNo: target.cardNo,
                    ...(target.cardNoState === undefined
                      ? {}
                      : { cardNoState: target.cardNoState }),
                    ...(target.intId === undefined ? {} : { intId: target.intId }),
                    ...routeState(target),
                  },
                  false,
                )
              }
            />
          )}
        </div>
      </div>
      {frame && navigation && (
        <div className="sticky bottom-0 z-20 mt-auto flex gap-2 border-t border-border bg-surface-1 px-4 py-3 pb-[max(env(safe-area-inset-bottom),0.75rem)]">
          {navigation}
        </div>
      )}
      <Dialog
        open={!pending && zoomedView !== undefined}
        onClose={closeZoom}
        title={zoomedView?.name?.primary.text ?? ""}
        variant="center"
      >
        {zoomedView?.faceSummary && zoomedView.name && (
          <CardImage
            summary={zoomedView.faceSummary}
            name={zoomedView.name.primary}
            images={images}
            alt="identify"
            sizes="(min-width: 600px) 512px, 100vw"
            className="mx-auto w-[min(100%,calc((92dvh-7rem)*63/88))]"
          />
        )}
      </Dialog>
    </article>
  )
}
