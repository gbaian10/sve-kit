import type { TFunction } from "i18next"
import {
  Camera,
  Check,
  ChevronRight,
  ClipboardPaste,
  Copy,
  Download,
  ImagePlus,
  Layers,
  Link2,
  Trash2,
  X,
} from "lucide-react"
import { type ReactNode, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react"
import { useTranslation } from "react-i18next"
import { Link } from "react-router"

import { deckImportPath, deckShareLink } from "../../app/deck-routes"
import { useDialogHistory } from "../../app/dialog-history"
import { routeState } from "../../app/listEntryState"
import { useRouteEdition } from "../../app/snapshot"
import { CardImage } from "../../components/card/CardImage"
import { ClassQuickBar } from "../../components/search/ClassQuickBar"
import { Button } from "../../components/ui/Button"
import { cn } from "../../components/ui/cn"
import { Dialog } from "../../components/ui/Dialog"
import { Segmented } from "../../components/ui/Segmented"
import {
  appScanModels,
  type ImageIndex,
  SCAN_PARTS,
  ScanModelError,
  type ScanModelStore,
} from "../../data"
import { printingPath } from "../../domain/cardRoute"
import {
  DECK_GUESS_GAP,
  type DeckCard,
  type DeckCardStatus,
  deckGroups,
  deckListText,
} from "../../domain/scan/deck-read"
import type { ScanCard } from "../../domain/scan/search"
import type { Region } from "../../domain/search"
import {
  cropUrl,
  type DeckProgress,
  drawDeckBoxes,
  PhotoError,
  photoFromBlob,
  readDeckPhoto,
} from "./deck-reader"
import { useScannedDeckShare } from "./deck-share"
import { openScanEngine, scanEngineSources } from "./engine"
import {
  classSummary,
  megabytes,
  type ScanResult,
  useScanClasses,
  useScanClassPick,
  useScanReadiness,
  useScanResults,
} from "./scan-hooks"
import { ScanModeTabs } from "./scan-shared"

const STATUSES: readonly DeckCardStatus[] = ["ok", "guess", "unsure"]
const BORDER: Record<DeckCardStatus | "off", string> = {
  ok: "border-scan-ok",
  guess: "border-scan-guess",
  unsure: "border-scan-unsure",
  off: "border-scan-off",
}
const DOT: Record<DeckCardStatus | "off", string> = {
  ok: "bg-scan-ok",
  guess: "bg-scan-guess",
  unsure: "bg-scan-unsure",
  off: "bg-scan-off",
}
const TEXT: Record<DeckCardStatus | "off", string> = {
  ok: "text-scan-ok",
  guess: "text-scan-guess",
  unsure: "text-scan-unsure",
  off: "text-scan-off",
}

type Message =
  | { readonly key: "loaded"; readonly width: number; readonly height: number }
  | { readonly key: "notImage" | "noClipboardImage" }
  | { readonly key: "cantDecode"; readonly type: string }
  | { readonly key: "camFailed" | "clipboardFailed"; readonly name: string }
  | { readonly key: "finding"; readonly pct: number }
  | { readonly key: "reading"; readonly done: number; readonly total: number }
  | { readonly key: "done"; readonly cards: number; readonly seconds: string }
  | { readonly key: "failed"; readonly message: string }
  | { readonly key: "confirmed"; readonly cards: number; readonly gap: string }

function messageText(t: TFunction, message: Message): string {
  switch (message.key) {
    case "loaded":
      return t("scan.deck.status.loaded", { width: message.width, height: message.height })
    case "notImage":
    case "noClipboardImage":
      return t(`scan.deck.status.${message.key}`)
    case "cantDecode":
      return t("scan.deck.status.cantDecode", { type: message.type })
    case "camFailed":
    case "clipboardFailed":
      return t(`scan.deck.status.${message.key}`, { name: message.name })
    case "finding":
      return t("scan.deck.status.finding", { pct: message.pct })
    case "reading":
      return t("scan.deck.status.reading", { done: message.done, total: message.total })
    case "done":
      return t("scan.deck.status.done", { cards: message.cards, seconds: message.seconds })
    case "failed":
      return t("scan.deck.status.failed", { message: message.message })
    case "confirmed":
      return t("scan.deck.status.confirmed", { cards: message.cards, gap: message.gap })
  }
}

type View = "number" | "card"
type ShareTab = "code" | "link" | "text"

// A trip to a card page and back unmounts the page; the photo and its list stay for the page load.
const kept: {
  photo: HTMLCanvasElement | null
  cards: (DeckCard | null)[] | null
  /** The card edition the list was read in; sharing and names follow it, not a later switch. */
  readEdition: Region
  view: View
  message: Message | null
} = { photo: null, cards: null, readEdition: "jp", view: "number", message: null }

function errorName(error: unknown): string {
  return error instanceof Error ? error.name : String(error)
}

export interface DeckScanPageProps {
  readonly store?: ScanModelStore
}

/** Deck-photo scanning: one photo of a laid-out deck becomes a card list to fix, share or import. */
export function DeckScanPage({ store = appScanModels }: DeckScanPageProps) {
  const { t } = useTranslation()
  const edition = useRouteEdition()
  const readiness = useScanReadiness(edition, store)
  const classes = useScanClasses()
  const [picked, toggleClass] = useScanClassPick()
  const [photo, setPhoto] = useState(kept.photo)
  const [cards, setCards] = useState(kept.cards)
  const [readEdition, setReadEdition] = useState<Region>(kept.readEdition)
  const [view, setView] = useState<View>(kept.view)
  const [message, setMessage] = useState<Message | null>(kept.message)
  const [dimmed, setDimmed] = useState<ReadonlySet<DeckCardStatus>>(new Set())
  const [open, setOpen] = useState<number | null>(null)
  // The reading in progress and the edition it reads; another edition makes it moot.
  const [busy, setBusy] = useState<{ readonly token: number; readonly edition: Region } | null>(
    null,
  )
  const reading = busy !== null && busy.edition === edition
  const [gap, setGap] = useState(DECK_GUESS_GAP)
  const [over, setOver] = useState(false)
  const [camera, setCamera] = useState<MediaStream | null>(null)
  const preview = useRef<HTMLCanvasElement>(null)
  const pickFile = useRef<HTMLInputElement>(null)
  const shootFile = useRef<HTMLInputElement>(null)
  const cameraVideo = useRef<HTMLVideoElement>(null)
  // A phone hands "take a photo" to its own camera app; a computer's file dialog cannot open the
  // webcam, so there the page shows the camera itself and takes one frame.
  const [touch] = useState(() => window.matchMedia("(pointer: coarse)").matches)

  useEffect(() => {
    kept.photo = photo
    kept.cards = cards
    kept.readEdition = readEdition
    kept.view = view
    kept.message = message
  })

  // Every async job (camera request, photo decode, reading) holds a token. A newer job, clearing
  // the photo, another edition or leaving the page makes older ones stale; stale jobs only clean
  // up after themselves.
  const mounted = useRef(true)
  const isMounted = () => mounted.current
  const readFlow = useRef(0)
  const photoFlow = useRef(0)
  const shootFlow = useRef(0)
  const cameraStream = useRef<MediaStream | null>(null)
  const releaseCamera = () => {
    shootFlow.current += 1
    for (const track of cameraStream.current?.getTracks() ?? []) track.stop()
    cameraStream.current = null
  }
  useEffect(() => {
    mounted.current = true
    return () => {
      mounted.current = false
      readFlow.current += 1
      photoFlow.current += 1
      releaseCamera()
    }
  }, [])
  // Another edition makes a reading in progress stale (its engine is released).
  useEffect(() => {
    readFlow.current += 1
  }, [edition])
  const stopCamera = () => {
    releaseCamera()
    setCamera(null)
  }
  const closeCamera = useDialogHistory("scan-camera", camera !== null, stopCamera)
  useEffect(() => {
    const video = cameraVideo.current
    if (!video || !camera) return
    video.srcObject = camera
    void video.play().catch(() => undefined)
  }, [camera])

  const dropReading = () => {
    readFlow.current += 1
    setBusy(null)
  }
  const take = async (blob: Blob | null | undefined) => {
    const token = ++photoFlow.current
    const isCurrent = () => isMounted() && photoFlow.current === token
    try {
      const next = await photoFromBlob(blob)
      if (!isCurrent()) return
      dropReading()
      setPhoto(next)
      setCards(null)
      setOpen(null)
      setMessage({ key: "loaded", width: next.width, height: next.height })
    } catch (error) {
      if (!isCurrent()) return
      if (error instanceof PhotoError)
        setMessage(
          error.kind === "not-image"
            ? { key: "notImage" }
            : { key: "cantDecode", type: error.type },
        )
      else setMessage({ key: "failed", message: errorName(error) })
    }
  }
  useEffect(() => {
    const onPaste = (event: ClipboardEvent) => {
      const item = [...(event.clipboardData?.items ?? [])].find((entry) =>
        entry.type.startsWith("image/"),
      )
      if (item) void take(item.getAsFile())
    }
    window.addEventListener("paste", onPaste)
    return () => {
      window.removeEventListener("paste", onPaste)
    }
    // take reads its tokens through refs.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])
  const paste = async () => {
    try {
      for (const item of await navigator.clipboard.read()) {
        const type = item.types.find((entry) => entry.startsWith("image/"))
        if (type) {
          await take(await item.getType(type))
          return
        }
      }
      setMessage({ key: "noClipboardImage" })
    } catch (error) {
      setMessage({ key: "clipboardFailed", name: errorName(error) })
    }
  }
  const shoot = async () => {
    if (touch) {
      shootFile.current?.click()
      return
    }
    const token = ++shootFlow.current
    try {
      const stream = await navigator.mediaDevices.getUserMedia({
        video: {
          facingMode: { ideal: "environment" },
          width: { ideal: 3840 },
          height: { ideal: 2160 },
        },
        audio: false,
      })
      // A later request, a closed window or a left page took over: this camera is not wanted.
      if (!isMounted() || shootFlow.current !== token) {
        for (const track of stream.getTracks()) track.stop()
        return
      }
      for (const track of cameraStream.current?.getTracks() ?? []) track.stop()
      cameraStream.current = stream
      setCamera(stream)
    } catch (error) {
      if (isMounted() && shootFlow.current === token)
        setMessage({ key: "camFailed", name: errorName(error) })
    }
  }
  const grab = () => {
    const video = cameraVideo.current
    if (!video || video.videoWidth === 0) return
    const frame = document.createElement("canvas")
    frame.width = video.videoWidth
    frame.height = video.videoHeight
    frame.getContext("2d")?.drawImage(video, 0, 0)
    closeCamera()
    frame.toBlob((blob) => void take(blob), "image/jpeg", 0.92)
  }
  const clearPhoto = () => {
    photoFlow.current += 1
    dropReading()
    setPhoto(null)
    setCards(null)
    setOpen(null)
    setMessage(null)
    if (pickFile.current) pickFile.current.value = ""
    if (shootFile.current) shootFile.current.value = ""
  }

  const read = async () => {
    if (!photo) return
    const token = ++readFlow.current
    // Read through a call: the photo, the edition or the page may change while this awaits.
    const isCurrent = () => isMounted() && readFlow.current === token
    const wanted = edition
    setBusy({ token, edition: wanted })
    setOpen(null)
    const started = performance.now()
    try {
      const engine = await openScanEngine(wanted, undefined, scanEngineSources(store))
      const found = await readDeckPhoto(engine, photo, picked, (progress: DeckProgress) => {
        if (!isCurrent()) return
        setMessage(
          progress.phase === "finding"
            ? { key: "finding", pct: progress.pct }
            : { key: "reading", done: progress.done, total: progress.total },
        )
      })
      if (!isCurrent()) return
      setCards(found)
      setReadEdition(wanted)
      setMessage({
        key: "done",
        cards: found.length,
        seconds: ((performance.now() - started) / 1000).toFixed(1),
      })
    } catch (error) {
      // A stale reading failing (its engine released by an edition switch) is not news.
      if (!isCurrent()) return
      setMessage(
        error instanceof ScanModelError && error.kind === "missing"
          ? null
          : { key: "failed", message: error instanceof Error ? error.message : String(error) },
      )
    } finally {
      if (isMounted()) setBusy((current) => (current?.token === token ? null : current))
    }
  }

  const live = useMemo(() => (cards ?? []).flatMap((card) => (card ? [card] : [])), [cards])
  const counts: Record<DeckCardStatus, number> = { ok: 0, guess: 0, unsure: 0 }
  for (const card of live) counts[card.status] += 1
  const groups = useMemo(() => deckGroups(cards ?? []), [cards])
  const deckName = t("scan.deck.share.deckName")
  const text = useMemo(() => deckListText(groups, deckName), [groups, deckName])
  const share = useScannedDeckShare(text, readEdition, deckName)

  // Names and pictures: picked cards, the best guess of unsure ones, and the open picker's candidates.
  const openCard = open === null ? null : (cards?.[open] ?? null)
  const closePicker = useDialogHistory("scan-picker", openCard !== null, () => {
    setOpen(null)
  })
  const shown = useMemo(() => {
    const list = live.flatMap((card) => {
      const first = card.pick ?? card.alts[0]?.card
      return first ? [first] : []
    })
    return [...list, ...(openCard?.alts.map((alt) => alt.card) ?? [])]
  }, [live, openCard])
  const { results, images } = useScanResults(shown, readEdition)
  const resultOf = useMemo(() => {
    const map = new Map(results.map((result) => [result.card.no, result]))
    return (card: ScanCard | undefined) => (card ? map.get(card.no) : undefined)
  }, [results])

  const colourOf = (status: DeckCardStatus) => (dimmed.has(status) ? "off" : status)
  useLayoutEffect(() => {
    if (!photo || !preview.current) return
    const style = getComputedStyle(preview.current)
    drawDeckBoxes(preview.current, photo, cards ?? [], (card) =>
      style.getPropertyValue(`--scan-${colourOf(card.status)}`).trim(),
    )
  })

  const choose = (at: number, choice: number | "remove") => {
    setCards((current) => {
      if (!current) return current
      const next = [...current]
      const card = next[at]
      if (!card) return current
      next[at] =
        choice === "remove"
          ? null
          : { ...card, pick: card.alts[choice]?.card ?? card.pick, status: "ok", gap: 0 }
      return next
    })
    setOpen(null)
  }
  const confirmGuesses = () => {
    if (!cards) return
    let confirmed = 0
    const next = cards.map((card) => {
      if (!card || card.status !== "guess" || card.gap > gap + 1e-9) return card
      confirmed += 1
      return { ...card, status: "ok" as const }
    })
    setCards(next)
    setMessage({ key: "confirmed", cards: confirmed, gap: gap.toFixed(2) })
  }

  const needsDownload = readiness.status !== null && !readiness.ready
  const source =
    "h-auto min-w-0 flex-col gap-1 px-1 py-3 text-12 leading-tight whitespace-nowrap md:text-13"

  return (
    <div className="mx-auto flex w-full max-w-240 flex-col gap-4 pt-4 pb-6">
      <h1 className="sr-only">{t("scan.deck.heading")}</h1>
      <div className="-mx-4 lg:mx-0">
        <ScanModeTabs mode="deck" />
      </div>

      <input
        ref={pickFile}
        type="file"
        accept="image/*"
        hidden
        onChange={(event) => void take(event.target.files?.[0])}
      />
      <input
        ref={shootFile}
        type="file"
        accept="image/*"
        capture="environment"
        hidden
        onChange={(event) => void take(event.target.files?.[0])}
      />
      {photo ? (
        <figure className="relative mx-auto w-fit max-w-full">
          <canvas ref={preview} className="block max-h-[65dvh] max-w-full rounded-button" />
          <figcaption className="sr-only">{t("scan.deck.photo")}</figcaption>
          <button
            type="button"
            onClick={clearPhoto}
            aria-label={t("scan.deck.clearPhoto")}
            title={t("scan.deck.clearPhoto")}
            className="absolute top-1 right-1 flex size-11 items-center justify-center rounded-full bg-camera-chip text-camera-text backdrop-blur"
          >
            <X className="size-5" aria-hidden="true" />
          </button>
        </figure>
      ) : (
        // Dropping a file is a pointer shortcut; the three buttons inside do the same by keyboard.
        // eslint-disable-next-line jsx-a11y/no-static-element-interactions
        <div
          onDragOver={(event) => {
            event.preventDefault()
            setOver(true)
          }}
          onDragLeave={() => {
            setOver(false)
          }}
          onDrop={(event) => {
            event.preventDefault()
            setOver(false)
            void take(event.dataTransfer.files[0])
          }}
          className={cn(
            "flex flex-col items-center gap-3 rounded-button border-2 border-dashed border-border-strong px-4 py-7 text-center",
            over ? "bg-accent-soft" : "bg-surface-1",
          )}
        >
          <Layers className="size-9 text-text-3" aria-hidden="true" />
          <div>
            <p className="text-16 font-bold">{t("scan.deck.intro")}</p>
            <p className="mt-1 text-13 text-text-2">{t("scan.deck.tips")}</p>
          </div>
          <div className="grid w-full max-w-md grid-cols-3 gap-2">
            <Button variant="primary" className={source} onClick={() => void shoot()}>
              <Camera className="size-6" aria-hidden="true" /> {t("scan.deck.shoot")}
            </Button>
            <Button className={source} onClick={() => pickFile.current?.click()}>
              <ImagePlus className="size-6" aria-hidden="true" /> {t("scan.deck.pick")}
            </Button>
            <Button className={source} onClick={() => void paste()}>
              <ClipboardPaste className="size-6" aria-hidden="true" /> {t("scan.deck.paste")}
            </Button>
          </div>
          {!touch && <p className="text-12 text-text-3">{t("scan.deck.dropHint")}</p>}
        </div>
      )}

      <div className="flex flex-col gap-1.5">
        {classes && (
          <ClassQuickBar
            label={t("search.classes")}
            classes={classes}
            selected={picked}
            onToggle={toggleClass}
          />
        )}
        <p className="text-12 text-text-3">
          {picked.length === 0
            ? t("scan.deck.classesNone")
            : t("scan.deck.classesSome", { classes: classSummary(picked, classes, t) })}
        </p>
      </div>

      {needsDownload && !readiness.downloading ? (
        <div className="flex flex-col gap-1.5">
          {readiness.status.manifest === "ok" && (
            <Button variant="primary" size="lg" className="w-full" onClick={readiness.download}>
              <Download className="size-5" aria-hidden="true" />
              {t("scan.single.download", { mb: megabytes(readiness.missingBytes) })}
            </Button>
          )}
          <p className="text-12 text-text-3">
            {t("scan.single.needModels", { mb: megabytes(readiness.missingBytes) })}{" "}
            <Link to="/cards/scan/models" className="text-accent-text underline">
              {t("scan.single.manage")}
            </Link>
          </p>
          {readiness.failed && (
            <p role="alert" className="text-12 text-danger">
              {t("scan.single.downloadFailed")}
            </p>
          )}
        </div>
      ) : readiness.downloading ? (
        <div className="flex flex-col gap-1.5">
          {SCAN_PARTS.map((part) => {
            const [got, total] = readiness.progress[part] ?? [0, 0]
            const pct = total ? Math.min(100, Math.round((got / total) * 100)) : 0
            return (
              <div key={part} className="flex flex-col gap-0.5">
                <div className="flex justify-between text-12 text-text-2">
                  <span>{t(`scan.models.${part}.name`)}</span>
                  <span className="tabular-nums">
                    {t("scan.single.progress", { got: megabytes(got), total: megabytes(total) })}
                  </span>
                </div>
                <div
                  role="progressbar"
                  aria-label={t(`scan.models.${part}.name`)}
                  aria-valuemin={0}
                  aria-valuemax={100}
                  aria-valuenow={pct}
                  className="h-1.5 overflow-hidden rounded-pill bg-surface-3"
                >
                  <div
                    className="h-full bg-accent transition-[width]"
                    style={{ width: `${String(pct)}%` }}
                  />
                </div>
              </div>
            )
          })}
        </div>
      ) : (
        <Button
          variant="primary"
          size="lg"
          className="w-full"
          disabled={!photo || !readiness.ready || reading}
          onClick={() => void read()}
        >
          {reading ? t("scan.deck.reading") : cards ? t("scan.deck.again") : t("scan.deck.start")}
        </Button>
      )}
      {message && (
        <p role="status" className="-mt-2 text-13 text-text-2">
          {messageText(t, message)}
        </p>
      )}

      {cards && (
        <>
          <div className="flex flex-col gap-3 rounded-button border border-border bg-surface-1 p-3">
            <p className="text-15">
              {t("scan.deck.total", {
                total: live.length,
                ok: live.length - counts.unsure,
                kinds: groups.length,
              })}
              {counts.unsure > 0 && (
                <span className={TEXT.unsure}>
                  {t("scan.deck.unsureCount", { cards: counts.unsure })}
                </span>
              )}
            </p>
            <div className="flex flex-wrap items-center justify-between gap-2">
              <Segmented
                label={t("scan.deck.view")}
                size="sm"
                value={view}
                onChange={setView}
                options={[
                  { value: "number", label: t("scan.deck.viewNumber") },
                  { value: "card", label: t("scan.deck.viewCard") },
                ]}
              />
              <div
                className="flex items-center gap-1.5"
                role="group"
                aria-label={t("scan.deck.colours")}
              >
                {STATUSES.map((status) => (
                  <button
                    key={status}
                    type="button"
                    aria-pressed={!dimmed.has(status)}
                    title={t("scan.deck.dimHint")}
                    onClick={() => {
                      const next = new Set(dimmed)
                      if (next.has(status)) next.delete(status)
                      else next.add(status)
                      setDimmed(next)
                    }}
                    className={cn(
                      "inline-flex h-11 items-center gap-1.5 rounded-control border border-border bg-surface-2 px-2.5 text-13 font-semibold tabular-nums md:h-8.5",
                      dimmed.has(status) && "text-text-3",
                    )}
                  >
                    <span
                      aria-hidden="true"
                      className={cn("size-2.5 rounded-full", DOT[colourOf(status)])}
                    />
                    {t(`scan.deck.state.${status}`)} {counts[status]}
                  </button>
                ))}
              </div>
            </div>
            {counts.guess > 0 && (
              <div className="flex flex-wrap items-center gap-2 border-t border-border pt-3 text-13 text-text-2">
                <span
                  aria-hidden="true"
                  className={cn("size-2.5 shrink-0 rounded-full", DOT.guess)}
                />
                <span>{t("scan.deck.guessRow", { cards: counts.guess })}</span>
                <input
                  type="number"
                  step="0.01"
                  min="0"
                  max="0.3"
                  value={gap}
                  onChange={(event) => {
                    setGap(Number(event.target.value))
                  }}
                  aria-label={t("scan.deck.gapLabel")}
                  className="h-11 w-16 rounded-control border border-border bg-surface-2 px-2 text-center text-text-1 tabular-nums md:h-8.5"
                />
                <Button size="sm" className="ml-auto h-11 md:h-8.5" onClick={confirmGuesses}>
                  <Check className="size-4" aria-hidden="true" /> {t("scan.deck.confirmAll")}
                </Button>
              </div>
            )}
          </div>

          <ResultGrid
            cards={cards}
            view={view}
            groups={groups}
            dimmed={dimmed}
            images={images}
            resultOf={resultOf}
            onOpen={setOpen}
          />

          <SharePanel
            text={share.text}
            code={share.code}
            missing={share.missing}
            edition={readEdition}
            unsure={counts.unsure}
          />
        </>
      )}

      <Dialog
        open={camera !== null}
        onClose={closeCamera}
        title={t("scan.deck.cameraHint")}
        variant="center"
      >
        <div className="flex flex-col gap-3">
          <video
            ref={cameraVideo}
            playsInline
            muted
            className="max-h-[70dvh] w-full rounded-control bg-camera object-contain"
          />
          <div className="flex justify-end gap-2">
            <Button onClick={closeCamera}>{t("scan.deck.cancelShot")}</Button>
            <Button variant="primary" onClick={grab}>
              <Camera className="size-5" aria-hidden="true" /> {t("scan.deck.shutter")}
            </Button>
          </div>
        </div>
      </Dialog>

      <Dialog
        open={openCard !== null}
        onClose={closePicker}
        title={t("scan.deck.picker.title")}
        variant="bottom"
        bare
      >
        {openCard && open !== null && (
          <Picker
            at={open}
            card={openCard}
            images={images}
            resultOf={resultOf}
            onChoose={(choice) => {
              closePicker()
              choose(open, choice)
            }}
            onClose={closePicker}
          />
        )}
      </Dialog>
    </div>
  )
}

function Badge({ at, status }: { readonly at: number; readonly status: DeckCardStatus | "off" }) {
  return (
    <span
      className={cn(
        "rounded-sm border px-1 text-12 font-bold tabular-nums",
        BORDER[status],
        TEXT[status],
      )}
    >
      #{at + 1}
    </span>
  )
}

function Thumb({
  result,
  images,
  className,
}: {
  readonly result: ScanResult | undefined
  readonly images: ImageIndex | undefined
  readonly className: string
}) {
  if (result?.faceImage)
    return (
      <img
        src={result.faceImage}
        alt=""
        className={cn("aspect-[63/88] rounded-sm bg-surface-3 object-cover", className)}
      />
    )
  if (!result?.summary || !result.name)
    return (
      <span
        aria-hidden="true"
        className={cn("aspect-[63/88] rounded-sm bg-surface-3", className)}
      />
    )
  return (
    <CardImage
      summary={result.summary}
      name={result.name.primary}
      images={images}
      alt="redundant"
      size="small"
      className={className}
    />
  )
}

const nameOf = (result: ScanResult | undefined, card: ScanCard | undefined) =>
  result?.name?.primary.text ?? card?.name ?? "—"

function ResultGrid({
  cards,
  view,
  groups,
  dimmed,
  images,
  resultOf,
  onOpen,
}: {
  readonly cards: readonly (DeckCard | null)[]
  readonly view: View
  readonly groups: ReturnType<typeof deckGroups>
  readonly dimmed: ReadonlySet<DeckCardStatus>
  readonly images: ImageIndex | undefined
  readonly resultOf: (card: ScanCard | undefined) => ScanResult | undefined
  readonly onOpen: (at: number) => void
}) {
  const { t } = useTranslation()
  const colour = (status: DeckCardStatus) => (dimmed.has(status) ? "off" : status)
  const item = (
    key: string,
    at: number,
    card: ScanCard | undefined,
    title: ReactNode,
    sub: ReactNode,
    status: DeckCardStatus,
  ) => (
    <button
      key={key}
      type="button"
      onClick={() => {
        onOpen(at)
      }}
      className={cn(
        "relative flex min-w-0 items-center gap-2 rounded-button border bg-surface-1 p-1.5 text-left",
        BORDER[colour(status)],
        dimmed.has(status) && "opacity-60",
      )}
    >
      <Thumb
        result={resultOf(card)}
        images={images}
        className={cn("w-11 shrink-0", dimmed.has(status) && "grayscale")}
      />
      <span className="min-w-0 flex-1">
        <span className="block truncate text-13 font-semibold">{title}</span>
        <span className="flex items-center gap-1 overflow-hidden text-12 whitespace-nowrap text-text-2">
          {sub}
        </span>
      </span>
    </button>
  )
  const maybe = (card: ScanCard | undefined) => (
    <>
      {t("scan.deck.unsureMark")}{" "}
      <span className="font-normal text-text-3">
        {t("scan.deck.maybe", { name: nameOf(resultOf(card), card) })}
      </span>
    </>
  )
  let items: ReactNode[]
  if (view === "number")
    items = cards.map((card, at) => {
      if (!card) return null
      const shown = card.status === "unsure" ? card.alts[0]?.card : (card.pick ?? undefined)
      return item(
        String(at),
        at,
        shown,
        card.status === "unsure" ? maybe(shown) : nameOf(resultOf(shown), shown),
        <>
          <Badge at={at} status={colour(card.status)} /> {card.status !== "unsure" && shown?.no}
          {card.status === "guess" && t("scan.deck.guessSub", { gap: card.gap.toFixed(2) })}
        </>,
        card.status,
      )
    })
  else {
    items = groups.map((group) => {
      const statuses = group.at.map((at) => cards[at]?.status ?? "ok")
      const status: DeckCardStatus = statuses.every((value) => value === "guess") ? "guess" : "ok"
      return item(
        group.card.cardId,
        group.at[0] ?? 0,
        group.card,
        <>
          {nameOf(resultOf(group.card), group.card)}{" "}
          <span className="text-accent-text">
            {t("scan.deck.copies", { count: group.at.length })}
          </span>
        </>,
        group.at.map((at, k) => <Badge key={at} at={at} status={colour(statuses[k] ?? "ok")} />),
        status,
      )
    })
    cards.forEach((card, at) => {
      if (card?.status !== "unsure") return
      items.push(
        item(
          `u${String(at)}`,
          at,
          card.alts[0]?.card,
          maybe(card.alts[0]?.card),
          <Badge at={at} status={colour("unsure")} />,
          "unsure",
        ),
      )
    })
  }
  return <div className="grid grid-cols-[repeat(auto-fill,minmax(10.5rem,1fr))] gap-2">{items}</div>
}

function Picker({
  at,
  card,
  images,
  resultOf,
  onChoose,
  onClose,
}: {
  readonly at: number
  readonly card: DeckCard
  readonly images: ImageIndex | undefined
  readonly resultOf: (card: ScanCard | undefined) => ScanResult | undefined
  readonly onChoose: (choice: number | "remove") => void
  readonly onClose: () => void
}) {
  const { t } = useTranslation()
  const crop = useMemo(() => cropUrl(card, 224, 312), [card])
  return (
    <div className="flex flex-col gap-3 p-4 pb-[max(1rem,env(safe-area-inset-bottom))]">
      <div className="contents">
        <div className="flex items-start gap-3">
          <img
            src={crop}
            alt={t("scan.deck.picker.photoAlt")}
            className="w-24 shrink-0 rounded-sm"
          />
          <div className="min-w-0 flex-1">
            <p className="flex items-center gap-1.5 text-15 font-bold">
              <Badge at={at} status={card.status} /> {t("scan.deck.picker.title")}
            </p>
            <p className="mt-1 text-13 text-text-2">{t("scan.deck.picker.hint")}</p>
          </div>
          <button
            type="button"
            aria-label={t("dialog.close")}
            onClick={onClose}
            className="flex size-11 shrink-0 items-center justify-center rounded-full bg-surface-2"
          >
            <X className="size-5" aria-hidden="true" />
          </button>
        </div>
        <div className="grid grid-cols-3 gap-2 md:grid-cols-6">
          {card.alts.map((alt, k) => {
            const result = resultOf(alt.card)
            const current = card.pick?.cardId === alt.card.cardId && card.status === "ok"
            return (
              <div
                key={alt.card.cardId}
                className={cn(
                  "flex flex-col gap-1 rounded-button border p-1.5",
                  current ? "border-accent bg-accent-soft" : "border-border",
                )}
              >
                <button
                  type="button"
                  onClick={() => {
                    onChoose(k)
                  }}
                  className="relative flex flex-col items-center gap-1 text-center"
                >
                  <Thumb result={result} images={images} className="w-full" />
                  <span className="line-clamp-2 text-12 leading-tight">
                    {nameOf(result, alt.card)}
                  </span>
                  <span className="text-11 text-text-3 tabular-nums">
                    {k === 0
                      ? t("scan.deck.picker.best", { score: alt.raw.toFixed(2) })
                      : alt.raw.toFixed(2)}
                  </span>
                </button>
                <Link
                  to={
                    result?.summary
                      ? printingPath(result.summary)
                      : printingPath({ cardNo: alt.card.no })
                  }
                  state={result?.summary ? routeState(result.summary) : undefined}
                  className="flex h-11 items-center justify-center gap-0.5 rounded-control border border-border text-12 text-text-2 hover:bg-surface-2 md:h-8"
                >
                  {t("scan.deck.picker.cardPage")}{" "}
                  <ChevronRight className="size-3.5" aria-hidden="true" />
                </Link>
              </div>
            )
          })}
        </div>
        <Button
          variant="danger"
          size="sm"
          className="h-11 self-start md:h-9"
          onClick={() => {
            onChoose("remove")
          }}
        >
          <Trash2 className="size-4" aria-hidden="true" /> {t("scan.deck.picker.notCard")}
        </Button>
      </div>
    </div>
  )
}

function SharePanel({
  text,
  code,
  missing,
  edition,
  unsure,
}: {
  readonly text: string
  readonly code: string
  readonly missing: readonly string[]
  readonly edition: "jp" | "en"
  readonly unsure: number
}) {
  const { t } = useTranslation()
  const [tab, setTab] = useState<ShareTab>("code")
  const [copied, setCopied] = useState<ShareTab | null>(null)
  const link = code ? deckShareLink(code) : ""
  const body = tab === "code" ? code : tab === "link" ? link : text
  // The builder reads a code (which names its edition) or the plain list in this edition.
  const builder = code
    ? deckImportPath(code)
    : `${deckImportPath(text)}${edition === "en" ? "&ed=en" : ""}`
  const copy = () => {
    void navigator.clipboard.writeText(body).then(() => {
      setCopied(tab)
      setTimeout(() => {
        setCopied((current) => (current === tab ? null : current))
      }, 1800)
    })
  }
  return (
    <section
      aria-labelledby="scan-share-title"
      className="flex flex-col gap-2 rounded-button border border-border bg-surface-1 p-3"
    >
      <h2 id="scan-share-title" className="text-15 font-bold">
        {t("scan.deck.share.title")}
      </h2>
      {unsure > 0 && (
        <p className="text-12 text-text-3">{t("scan.deck.share.unsureNote", { cards: unsure })}</p>
      )}
      {text ? (
        <Link
          to={builder}
          className="flex h-12 items-center justify-center gap-2 rounded-button bg-accent px-5 text-15 font-semibold text-accent-ink"
        >
          {t("scan.deck.share.openBuilder")} <ChevronRight className="size-5" aria-hidden="true" />
        </Link>
      ) : null}
      <Segmented
        label={t("scan.deck.share.tabs")}
        value={tab}
        onChange={setTab}
        options={[
          { value: "code", label: t("scan.deck.share.code") },
          { value: "link", label: t("scan.deck.share.link") },
          { value: "text", label: t("scan.deck.share.text") },
        ]}
      />
      <div className="relative rounded-control border border-border bg-surface-2 p-3 pr-24">
        <Button
          size="sm"
          variant={copied === tab ? "primary" : "secondary"}
          disabled={!body}
          onClick={copy}
          className="absolute top-2 right-2 h-11 md:h-9"
        >
          {copied === tab ? (
            <Check className="size-4" aria-hidden="true" />
          ) : tab === "link" ? (
            <Link2 className="size-4" aria-hidden="true" />
          ) : (
            <Copy className="size-4" aria-hidden="true" />
          )}
          {copied === tab ? t("scan.deck.share.copied") : t("scan.deck.share.copy")}
        </Button>
        <p className="mb-2 text-12 text-text-3">{t(`scan.deck.share.${tab}Hint`)}</p>
        {body ? (
          <pre className="max-h-60 overflow-auto font-mono text-12 break-all whitespace-pre-wrap">
            {body}
          </pre>
        ) : (
          <p className="text-13 text-text-3">
            {missing.length > 0
              ? t("scan.deck.share.missing", { cards: missing.join(t("scan.listSeparator")) })
              : t("scan.deck.share.preparing")}
          </p>
        )}
      </div>
    </section>
  )
}
