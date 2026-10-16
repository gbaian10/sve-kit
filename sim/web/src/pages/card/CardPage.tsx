import { ArrowLeft, ChevronLeft, ChevronRight, Link2 } from "lucide-react"
import { useEffect, useMemo, useState } from "react"
import { useTranslation } from "react-i18next"
import { Link as RouterLink, useLocation, useNavigate, useParams } from "react-router"

import { type CardContent, prepareCardContent } from "../../app/card-content"
import { useDialogHistory } from "../../app/dialog-history"
import { LIST_PAGE_SIZE, routeState } from "../../app/listEntryState"
import { useOtherEdition } from "../../app/other-edition"
import {
  type SearchReadiness,
  useCardSnapshot,
  useImageIndex,
  useNeighbours,
  useRouteEdition,
  useSearchReadiness,
} from "../../app/snapshot"
import { StatusStrip } from "../../app/StatusStrip"
import { CardImage } from "../../components/card/CardImage"
import { CardNameHeading, PlainNameHeading } from "../../components/card/CardNameHeading"
import { CLASS_ICON } from "../../components/card/classIcons"
import { StatLine } from "../../components/card/StatLine"
import { useTextLanguage } from "../../components/card/text-language"
import { type VocabularyKind, vocabularyLabel } from "../../components/card/vocabulary"
import { Badge } from "../../components/ui/Badge"
import { Button } from "../../components/ui/Button"
import { cn } from "../../components/ui/cn"
import { Dialog } from "../../components/ui/Dialog"
import { Link } from "../../components/ui/Link"
import { useToast } from "../../components/ui/toast-context"
import {
  cardPrintings,
  type CardSummary,
  type Catalog,
  faceBasics,
  type ImageIndex,
  locateCard,
  type LocatedCard,
  nameOf,
  type Row,
  type SnapshotClient,
  type SnapshotStatus,
  stringValue,
} from "../../data"
import { CardRouteError, printingPath } from "../../domain/cardRoute"
import { classStyle } from "../../domain/classes"
import { displayName, UI_TEXT_LANG } from "../../domain/nameDisplay"
import { parseQuery } from "../../domain/query/codec"
import type { Region } from "../../domain/search"
import { currentUiLanguage } from "../../i18n"
import { showsOtherEdition, usePrefs } from "../../settings"
import { PageTitle } from "../PageTitle"
import { CardTextPanel } from "./CardTextPanel"
import { CardWording } from "./CardWording"
import { printingGroups } from "./printing-order"

const ICON_BUTTON =
  "flex size-11 shrink-0 items-center justify-center rounded-button text-text-1 hover:bg-surface-2"
// Phone: 72% of the width, as the reference sites do; tablet and desktop: the 340px column.
const IMAGE_SLOT = "w-[72%] max-w-85 lg:w-full lg:max-w-none"
const IMAGE_SIZES = "(min-width: 600px) 340px, 72vw"
// Each prepared card keeps its own catalog and text closure (several MB), so only nearby cards stay.
const PREPARED_CARDS = 4

export function CardPage() {
  const { cardNo, intId } = useParams()
  const { client, status } = useCardSnapshot()
  const edition = useRouteEdition()
  const location = useLocation()
  const [attempt, setAttempt] = useState(0)
  const language = useTextLanguage()
  const [prepared, setPrepared] = useState(() => new Map<string, CardContent>())
  const routeKey = JSON.stringify((location.state as { route?: unknown } | null)?.route ?? null)
  const inputKey = JSON.stringify([
    client.base,
    client.snapshot()?.manifestHash,
    edition,
    language,
    routeKey,
    attempt,
    cardNo === undefined ? "provisional" : "official",
    cardNo ?? intId ?? "",
  ])
  const cached = prepared.get(inputKey)
  const [located, setLocated] = useState<{
    key: string
    value?: CardContent
    error?: unknown
  } | null>(null)
  if (cached && located?.key !== inputKey) setLocated({ key: inputKey, value: cached })
  const manifestHash = client.snapshot()?.manifestHash
  useEffect(() => {
    if (!manifestHash || cached) return
    let active = true
    const hint = JSON.parse(routeKey) as {
      cardId: string
      printingId: string
      cardOwner: string
      printingOwner: string
      region: Region
    } | null
    const loading = locateCard(client, {
      namespace: cardNo === undefined ? "provisional" : "official",
      key: cardNo ?? intId ?? "",
      edition,
      ...(hint ? { hint: { ...hint, id: hint.printingId } } : {}),
    }).then((value) => prepareCardContent(client, value, language, IMAGE_SIZES))
    void loading.then(
      (value) => {
        if (active) {
          // A failed image index stays uncached so returning to the card retries it.
          if (!value.images.failed)
            setPrepared((previous) => {
              const next = new Map(previous).set(inputKey, value)
              if (next.size > PREPARED_CARDS) next.delete(next.keys().next().value ?? "")
              return next
            })
          setLocated({ key: inputKey, value })
        }
      },
      (error: unknown) => {
        if (active) setLocated({ key: inputKey, error })
      },
    )
    return () => {
      active = false
    }
  }, [client, manifestHash, inputKey, cardNo, intId, routeKey, attempt, edition, language, cached])
  const current = cached !== undefined || located?.key === inputKey
  const result = cached
    ? { key: inputKey, value: cached }
    : current ||
        (located?.value?.snapshot.manifestHash === manifestHash &&
          located?.value?.client.base === client.base)
      ? located
      : null
  const pending = !current
  const [pendingHint, setPendingHint] = useState<string | null>(null)
  useEffect(() => {
    if (!pending) return
    const timer = setTimeout(() => {
      setPendingHint(inputKey)
    }, 200)
    return () => {
      clearTimeout(timer)
    }
  }, [pending, inputKey])
  const catalog = result?.value?.catalog ?? null
  const routeError = result?.error
  const resolved = result?.value?.printing
  return (
    <CardView
      client={result?.value?.client ?? client}
      preparedImages={result?.value?.images}
      pending={pending}
      showPending={pendingHint === inputKey}
      status={status}
      catalog={catalog}
      resolved={resolved}
      routeError={routeError}
      onRetry={() => {
        void client.reload().then(() => {
          setAttempt((value) => value + 1)
        })
      }}
    />
  )
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
}: {
  readonly client: SnapshotClient
  readonly status: SnapshotStatus
  readonly catalog: Catalog | null
  readonly resolved: LocatedCard["printing"] | undefined
  readonly onRetry?: () => void
  readonly routeError?: unknown
  readonly pending?: boolean
  readonly showPending?: boolean
  readonly preparedImages?: ImageIndex | undefined
}) {
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
    typeof location.state === "object" && location.state !== null
      ? (location.state as Record<string, unknown>)
      : null
  const background = typeof entry?.["background"] === "string" ? entry["background"] : null
  const fromList = background !== null
  const pages = typeof entry?.["pages"] === "number" ? Math.max(1, entry["pages"]) : 1
  const neighbourQuery = useMemo(
    () => ({ state: parseQuery(new URLSearchParams(background ?? "")), edition: cardEdition }),
    [background, cardEdition],
  )
  const hint = entry?.["route"] as { printingId?: string } | undefined
  const neighbourTarget =
    typeof entry?.["resultKey"] === "string"
      ? entry["resultKey"]
      : (hint?.printingId ?? (pending ? null : printingId))
  const neighbours = useNeighbours(
    fromList ? neighbourQuery : null,
    fromList ? neighbourTarget : null,
    fromList ? pages * LIST_PAGE_SIZE : undefined,
  )
  const order = neighbours.result.phase === "complete" ? neighbours.result.page.items : []
  const position = order.findIndex(
    (item) => item.key === neighbourTarget || item.printingId === neighbourTarget,
  )
  const neighbour = (step: -1 | 1) => {
    const item = position < 0 ? undefined : order[position + step]
    return item?.summary ? { key: item.key, summary: item.summary } : undefined
  }
  const previous = fromList ? neighbour(-1) : undefined
  const next = fromList ? neighbour(1) : undefined
  const canonical = summary
    ? summary.cardNoState === "provisional"
      ? printingPath(summary)
      : `${printingPath(summary)}/${encodeURIComponent(summary.name.original.text)}`
    : null
  useEffect(() => {
    if (!pending && canonical && location.pathname !== canonical)
      void navigate(canonical, { replace: true, state: location.state as unknown })
  }, [pending, canonical, summary?.cardNoState, location.pathname, location.state, navigate])
  const open = (target: NonNullable<typeof previous>) => {
    void navigate(printingPath(target.summary), {
      replace: true,
      state: { ...entry, resultKey: target.key, ...routeState(target.summary) },
    })
  }
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey) return
      const target = event.target
      if (
        target instanceof Element &&
        target.closest("input, textarea, select, [contenteditable=true], dialog") !== null
      )
        return
      if (event.key === "ArrowLeft" && previous) open(previous)
      else if (event.key === "ArrowRight" && next) open(next)
      else return
      event.preventDefault()
    }
    window.addEventListener("keydown", onKeyDown)
    return () => {
      window.removeEventListener("keydown", onKeyDown)
    }
  })
  const copyLink = async () => {
    try {
      await navigator.clipboard.writeText(window.location.href)
      toast.show({ message: t("cardPage.linkCopied") })
    } catch {
      toast.show({ message: t("cardPage.linkCopyFailed") })
    }
  }
  const index = catalog?.index
  const label = (kind: VocabularyKind, code: string) =>
    index ? vocabularyLabel(index, kind, code, textLang) : code
  const joiner = t("cardPage.separator")
  const metaLine = (classCode: string | null | undefined, typeCode: string | null) =>
    [
      ...(classCode === undefined
        ? []
        : [classCode === null ? t("search.neutral") : label("class", classCode)]),
      ...(typeCode === null ? [] : [label("type", typeCode)]),
    ].join(joiner)
  const traitLabels = (codes: readonly string[]) =>
    codes.map((code) => label("trait", code)).join(joiner)

  if (routeError || (catalog !== null && !printing)) {
    const kind = routeError instanceof CardRouteError ? routeError.kind : undefined
    // A key the route contract rejects names no card, the same as an unknown number.
    const missing = kind === "not-found" || kind === "invalid"
    return (
      <>
        <PageTitle>
          {kind === "needs-update"
            ? t("cardPage.needsUpdate")
            : missing
              ? t("cardPage.notFound")
              : t("cardPage.routeFailed")}
        </PageTitle>
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
    const revision = faceCatalog?.index.displayRevision(faceId, region)
    const nameRow = revision ?? (wording?.["common_name"] as Row | null | undefined)
    const backName =
      ordinal > 0 && nameRow && faceCatalog ? nameOf(faceCatalog.index, nameRow) : undefined
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
            wordingPending: wording !== undefined,
          })
    const name = faceSummary
      ? displayName(faceSummary.name, uiLanguage, prefs.nameDisplay)
      : undefined
    return { faceId, ordinal, wording, revision, faceSummary, name }
  })
  const zoomedView = views.find((view) => view.faceId === zoomed)
  return (
    <article aria-busy={pending} className="relative mx-auto w-full max-w-270">
      {pending && showPending && catalog && (
        <p
          role="status"
          className="pointer-events-none absolute top-13 right-0 z-10 rounded-control bg-surface-2 px-3 py-1 text-13 text-text-2 shadow"
        >
          {t("cardPage.switchingCard")}
        </p>
      )}
      <StatusStrip readiness={readiness} />
      {/* Top row (design 06): back 44, set tag 28, number centred in Plex 600 15px, copy link 44. */}
      <div className="flex h-13 items-center gap-1">
        {fromList ? (
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
        <h1 className="min-w-0 flex-1 truncate text-center font-sans text-15 font-semibold tabular-nums">
          {resolved?.cardNo ?? cardNo ?? intId}
        </h1>
        <button
          type="button"
          aria-label={t("cardPage.copyLink")}
          title={t("cardPage.copyLink")}
          onClick={() => void copyLink()}
          className={ICON_BUTTON}
        >
          <Link2 className="size-5" aria-hidden="true" />
        </button>
      </div>
      {/* Phone and tablet: the large image first, then the text; desktop: a sticky image column. */}
      <div
        className="lg:grid lg:grid-cols-[340px_minmax(0,1fr)] lg:gap-x-10"
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
        <div className="mt-1 flex flex-col items-center gap-3 lg:sticky lg:top-4 lg:self-start">
          {views.map(
            ({ faceId, faceSummary, name }) =>
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
                    IMAGE_SLOT,
                  )}
                >
                  <CardImage
                    summary={faceSummary}
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
            const traits = traitLabels((values?.["traits"] ?? []) as string[])
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
                    {(classCode !== undefined || typeCode !== null || traits !== "") && (
                      <p className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-13 text-text-2">
                        {classCode !== undefined && (
                          <img
                            src={CLASS_ICON[classStyle(classCode)]}
                            alt=""
                            className="size-4.5"
                          />
                        )}
                        <span>{metaLine(classCode, typeCode)}</span>
                        {traits !== "" && <span className="text-text-3">{traits}</span>}
                      </p>
                    )}
                    {(cost !== null || attack !== null || defense !== null) && (
                      <StatLine className="mt-1" cost={cost} attack={attack} defense={defense} />
                    )}
                    {fromList &&
                      ordinal === 0 &&
                      (neighbours.result.phase !== "complete" || position >= 0) && (
                        <div className="mt-2 flex gap-2">
                          <Button
                            data-card-navigation
                            className="h-10 gap-1 px-3 text-14"
                            disabled={previous === undefined}
                            onClick={() => {
                              if (previous) open(previous)
                            }}
                          >
                            <ChevronLeft className="size-4.5" aria-hidden="true" />
                            {t("cardPage.previous")}
                          </Button>
                          <Button
                            data-card-navigation
                            className="h-10 gap-1 px-3 text-14"
                            disabled={next === undefined}
                            onClick={() => {
                              if (next) open(next)
                            }}
                          >
                            {t("cardPage.next")}
                            <ChevronRight className="size-4.5" aria-hidden="true" />
                          </Button>
                        </div>
                      )}
                  </div>
                )}
                {printingId && catalog && (
                  <CardTextPanel
                    key={printingId}
                    client={client}
                    edition={region}
                    printingId={printingId}
                    faceId={faceId}
                  >
                    <CardWording
                      client={client}
                      edition={region}
                      catalog={catalog}
                      printingId={printingId}
                      wording={wording}
                      revision={revision}
                      entry={entry}
                    />
                  </CardTextPanel>
                )}
              </section>
            )
          })}
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
              {printingGroups(printings).map(
                ({ region: printingRegion, printings: regionalPrintings }) => (
                  <div key={printingRegion} className="mt-1.5 flex items-start gap-2">
                    <span className="inline-flex h-8 items-center text-13 font-semibold text-text-3">
                      {printingRegion.toUpperCase()}:
                    </span>
                    <ul aria-label={printingRegion.toUpperCase()} className="flex flex-wrap gap-2">
                      {regionalPrintings.map((row) => {
                        const id = stringValue(row["id"])
                        const no = stringValue(row["card_no"])
                        const path = printingPath({
                          cardNo: no,
                          ...(typeof row["card_no_state"] === "string"
                            ? { cardNoState: row["card_no_state"] }
                            : {}),
                          ...(typeof row["int_id"] === "number" ? { intId: row["int_id"] } : {}),
                        })
                        const rarity =
                          typeof row["rarity_code"] === "string"
                            ? label("rarity", row["rarity_code"])
                            : row["rarity_raw"]
                        return (
                          <li key={id}>
                            <RouterLink
                              to={path}
                              state={{
                                ...entry,
                                route: {
                                  cardId: stringValue(row["card_id"]),
                                  printingId: id,
                                  cardOwner: stringValue(card?.["home_set_id"]),
                                  printingOwner: stringValue(row["home_set_id"]),
                                  region: printingRegion,
                                },
                              }}
                              replace
                              aria-current={id === printingId ? "page" : undefined}
                              className="inline-flex h-8 items-center rounded-pill border border-border px-2.5 font-mono text-13 hover:bg-surface-2 aria-[current=page]:border-accent aria-[current=page]:bg-accent-soft aria-[current=page]:font-semibold"
                            >
                              {no}
                              {typeof rarity === "string" && rarity !== "" && ` (${rarity})`}
                            </RouterLink>
                          </li>
                        )
                      })}
                    </ul>
                  </div>
                ),
              )}
            </section>
          )}
        </div>
      </div>
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
