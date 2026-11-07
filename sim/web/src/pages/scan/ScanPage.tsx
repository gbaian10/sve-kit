import {
  Camera,
  Check,
  ChevronRight,
  Download,
  History,
  Lock,
  LockOpen,
  RectangleHorizontal,
  RectangleVertical,
  ScanLine,
} from "lucide-react"
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react"
import { useTranslation } from "react-i18next"
import { Link } from "react-router"

import { routeState } from "../../app/listEntryState"
import { useRouteEdition } from "../../app/snapshot"
import { CardImage } from "../../components/card/CardImage"
import { Button } from "../../components/ui/Button"
import { cn } from "../../components/ui/cn"
import {
  appScanModels,
  type ImageIndex,
  SCAN_PARTS,
  ScanModelError,
  type ScanModelStore,
} from "../../data"
import { printingPath } from "../../domain/cardRoute"
import type { Region } from "../../domain/search"
import { type CameraFrame, CameraScanner } from "./camera"
import {
  openScanEngine,
  scanEngineSources,
  type ScanEngineStage,
  ScanEngineStartError,
} from "./engine"
import {
  classSummary,
  megabytes,
  type ScanResult,
  useScanClasses,
  useScanClassPick,
  useScanReadiness,
  useScanResults,
} from "./scan-hooks"
import { ClassColumn, ScanModeTabs } from "./scan-shared"

// Card proportions (63 × 88 mm) for the aiming frame.
const CARD_RATIO = 88 / 63
const ZOOMS = [1, 1.5, 2, 3]

// Coming back from a card page should not ask for the camera again: remember it was on.
let cameraWasOn = false

type Phase = "idle" | "starting" | "running" | "error"

export interface ScanPageProps {
  readonly store?: ScanModelStore
}

/** Single-card scanning: aim the camera at one card and see which card it is. */
export function ScanPage({ store = appScanModels }: ScanPageProps) {
  const { t } = useTranslation()
  const edition = useRouteEdition()
  const readiness = useScanReadiness(edition, store)
  const classes = useScanClasses()
  const [picked, toggleClass] = useScanClassPick()
  const [sideways, setSideways] = useState(false)
  const [paused, setPaused] = useState(false)
  const [phase, setPhase] = useState<Phase>("idle")
  const [error, setError] = useState("")
  const [stage, setStage] = useState<ScanEngineStage | null>(null)
  const [frame, setFrame] = useState<CameraFrame | null>(null)
  const [zoom, setZoom] = useState(1)
  const [maxZoom, setMaxZoom] = useState(4)
  const scanner = useRef<CameraScanner | null>(null)
  const video = useRef<HTMLVideoElement>(null)
  const aim = useRef<HTMLDivElement>(null)
  const area = useRef<HTMLDivElement>(null)
  const overlay = useRef<HTMLCanvasElement>(null)
  const [aimSize, setAimSize] = useState({ width: 0, height: 0 })

  const sources = useMemo(() => scanEngineSources(store), [store])
  // Cleared on unmount: a camera start still waiting for permission must not go on afterwards.
  const mounted = useRef(true)
  useEffect(() => {
    mounted.current = true
    return () => {
      mounted.current = false
      scanner.current?.stop()
      scanner.current = null
    }
  }, [])
  const fail = (cause: unknown) => {
    scanner.current?.stop()
    scanner.current = null
    // Models missing for this edition (e.g. just switched to one not downloaded): offer the download.
    if (cause instanceof ScanModelError && cause.kind === "missing") {
      setError("")
      setPhase("idle")
      return
    }
    // WebGPU could not start here: say so, and point at the CPU build, which always works.
    if (cause instanceof ScanEngineStartError && cause.backend === "webgpu") {
      setError(t("scan.engine.webgpuFailed"))
    } else setError(cause instanceof Error ? cause.message : String(cause))
    setPhase("error")
  }
  // Async flows (camera start, edition switch) each hold a token; a newer flow or leaving the page
  // makes older ones stale, and a stale flow only cleans up after itself.
  const startFlow = useRef(0)
  const switchFlow = useRef(0)
  const latestEdition = useRef(edition)
  const shownEdition = useRef(edition)
  // The camera lets go of the old engine first, since opening another edition releases it, then
  // picks up the new one.
  const switchTo = (target: Region) => {
    const current = scanner.current
    if (!current || shownEdition.current === target) return
    shownEdition.current = target
    const token = ++switchFlow.current
    const stillWanted = () => switchFlow.current === token && scanner.current === current
    current.detach()
    void openScanEngine(target, undefined, sources).then(
      (engine) => {
        if (stillWanted()) current.setEngine(engine)
      },
      (cause: unknown) => {
        if (stillWanted()) fail(cause)
      },
    )
  }
  // The account menu's edition applies at once.
  useEffect(() => {
    latestEdition.current = edition
    switchTo(edition)
    // switchTo reads the latest scanner and sources itself.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [edition])
  useEffect(() => scanner.current?.setClasses(picked), [picked])
  useEffect(() => scanner.current?.setSideways(sideways), [sideways])

  useLayoutEffect(() => {
    const element = area.current
    if (!element) return
    const fit = () => {
      const { width, height } = element.getBoundingClientRect()
      if (!sideways) {
        const h = Math.min(height * 0.84, width * 0.62 * CARD_RATIO)
        setAimSize({ width: h / CARD_RATIO, height: h })
      } else {
        const w = Math.min(width * 0.88, height * 0.8 * CARD_RATIO)
        setAimSize({ width: w, height: w / CARD_RATIO })
      }
    }
    fit()
    const observer = new ResizeObserver(fit)
    observer.observe(element)
    return () => {
      observer.disconnect()
    }
  }, [sideways])

  const start = async () => {
    const [videoElement, aimElement, overlayElement] = [video.current, aim.current, overlay.current]
    if (!videoElement || !aimElement || !overlayElement) return
    const token = ++startFlow.current
    switchFlow.current += 1
    // Read through a call: the page may unmount or start again while this awaits.
    const isCurrent = () => mounted.current && startFlow.current === token
    scanner.current?.stop()
    scanner.current = null
    setPhase("starting")
    setError("")
    const wanted = edition
    let camera: CameraScanner | null = null
    try {
      const engine = await openScanEngine(wanted, setStage, sources)
      if (!isCurrent()) return
      camera = new CameraScanner(engine)
      camera.attach(videoElement, aimElement, overlayElement)
      camera.setClasses(picked)
      camera.setSideways(sideways)
      scanner.current = camera
      shownEdition.current = wanted
      const zoomLimit = await camera.start()
      // Whoever stopped the camera meanwhile (a newer start, a failed edition switch) took it away.
      if (!isCurrent() || scanner.current !== camera) {
        camera.stop()
        return
      }
      setMaxZoom(zoomLimit)
      cameraWasOn = true
      setPaused(false)
      setPhase("running")
      const running = camera
      running.run(setFrame).catch((cause: unknown) => {
        if (scanner.current === running) fail(cause)
      })
      // The edition changed while the camera was starting.
      switchTo(latestEdition.current)
    } catch (cause) {
      if (!isCurrent() || (camera !== null && scanner.current !== camera)) {
        camera?.stop()
        return
      }
      fail(cause)
    }
  }
  const startOnReturn = useRef(true)
  const ready = readiness.ready
  useEffect(() => {
    if (!startOnReturn.current || !ready || !cameraWasOn) return
    // The camera is an outside system: turn it back on from a task, not during this render pass.
    const timer = setTimeout(() => {
      startOnReturn.current = false
      void start()
    })
    return () => {
      clearTimeout(timer)
    }
    // Only the first time the models are known to be ready; start reads the latest refs itself.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready])

  const togglePause = () => {
    if (phase !== "running") return
    scanner.current?.setPaused(!paused)
    setPaused(!paused)
  }

  const picks = frame?.picks ?? []
  const shown = [...picks.map((pick) => pick.card), ...(frame?.recent ?? [])]
  const { results, images } = useScanResults(shown, edition)
  const status = paused ? "locked" : (frame?.status ?? "unsure")
  const [top, ...others] = results.slice(0, picks.length)
  const recent = results.slice(picks.length)

  return (
    // Inside the site frame: the camera and result sheet fill the height down to the bottom tabs
    // (edge to edge on a phone, a centred panel on a wide screen).
    <div className="-mx-4 flex h-[calc(100dvh-3.25rem-5.25rem)] flex-col gap-3 overflow-x-clip pt-3 lg:mx-auto lg:h-[calc(100dvh-3.25rem)] lg:w-full lg:max-w-240 lg:pb-4 phone-landscape:h-[calc(100dvh-3.25rem)]">
      <h1 className="sr-only">{t("scan.single.heading")}</h1>
      <ScanModeTabs
        mode="single"
        start={
          <button
            type="button"
            onClick={() => {
              setSideways(!sideways)
            }}
            aria-pressed={sideways}
            className="relative flex h-9 items-center gap-1 rounded-pill bg-surface-2 px-3 text-13 text-text-1 before:absolute before:inset-x-0 before:-inset-y-1 before:content-[''] hover:bg-surface-3"
          >
            {sideways ? (
              <RectangleHorizontal className="size-4" aria-hidden="true" />
            ) : (
              <RectangleVertical className="size-4" aria-hidden="true" />
            )}
            {sideways ? t("scan.landscape") : t("scan.portrait")}
          </button>
        }
      />
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden bg-camera text-camera-text lg:rounded-button">
        <div className="relative min-h-0 flex-1 overflow-hidden">
          {}
          <video
            ref={video}
            playsInline
            muted
            onClick={togglePause}
            className="absolute inset-0 size-full object-cover"
          />
          <div
            ref={area}
            className="pointer-events-none absolute inset-x-0 top-3 bottom-12 flex items-center justify-center"
          >
            <div
              ref={aim}
              style={{
                width: aimSize.width,
                height: aimSize.height,
                boxShadow: "0 0 0 9999px var(--camera-dim)",
              }}
              className={cn(
                "rounded-block border-2 transition-colors",
                phase !== "running" && "opacity-0",
                status === "ok" ? "border-camera-ok" : "border-camera-frame",
              )}
            />
          </div>
          <canvas ref={overlay} className="pointer-events-none absolute inset-0 size-full" />

          {phase === "running" && (
            <div className="pointer-events-none absolute inset-x-0 bottom-3 flex justify-center">
              <span
                role="status"
                className={cn(
                  "flex items-center gap-1.5 rounded-pill px-3 py-1 text-13 backdrop-blur",
                  status === "ok"
                    ? "bg-camera-ok-pill"
                    : status === "locked"
                      ? "bg-camera-lock-pill"
                      : "bg-camera-chip",
                )}
              >
                {status === "ok" && <Check className="size-4" aria-hidden="true" />}
                {status === "locked" && <Lock className="size-3.5" aria-hidden="true" />}
                {status === "ok"
                  ? t("scan.single.recognized")
                  : status === "busy"
                    ? t("scan.single.busy")
                    : status === "locked"
                      ? t("scan.single.locked")
                      : frame?.found
                        ? t("scan.single.unsure")
                        : picked.length > 0
                          ? t("scan.single.aimFiltered", {
                              classes: classSummary(picked, classes, t),
                            })
                          : t("scan.single.aim")}
              </span>
            </div>
          )}

          {phase !== "running" && (
            <StartPanel
              readiness={readiness}
              phase={phase}
              stage={stage}
              error={error}
              onStart={() => void start()}
            />
          )}

          {phase === "running" && (
            <button
              type="button"
              aria-pressed={paused}
              aria-label={t("scan.single.lock")}
              title={t("scan.single.lock")}
              onClick={togglePause}
              className="absolute top-2 right-2 flex size-11 items-center justify-center"
            >
              <span
                className={cn(
                  "flex size-10 items-center justify-center rounded-full backdrop-blur",
                  paused
                    ? "bg-camera-lock-pill text-camera-text"
                    : "bg-camera-chip text-camera-text",
                )}
              >
                {paused ? (
                  <Lock className="size-5" aria-hidden="true" />
                ) : (
                  <LockOpen className="size-5" aria-hidden="true" />
                )}
              </span>
            </button>
          )}

          {phase === "running" && (
            <div className="absolute top-1/2 left-2 -translate-y-1/2">
              <ClassColumn picked={picked} onToggle={toggleClass} />
            </div>
          )}

          {phase === "running" && (
            <div
              role="group"
              aria-label={t("scan.single.zoom")}
              className="absolute top-1/2 right-2 flex -translate-y-1/2 flex-col gap-1"
            >
              {ZOOMS.filter((step) => step <= maxZoom).map((step) => (
                <button
                  key={step}
                  type="button"
                  aria-pressed={zoom === step}
                  onClick={() => {
                    setZoom(step)
                    void scanner.current?.setZoom(step)
                  }}
                  // A 44px target around a 40px dial (design §9 touch size).
                  className="flex size-11 items-center justify-center"
                >
                  <span
                    className={cn(
                      "flex size-10 items-center justify-center rounded-full text-12 font-semibold tabular-nums backdrop-blur",
                      zoom === step
                        ? "bg-camera-text text-camera"
                        : "bg-camera-chip text-camera-text",
                    )}
                  >
                    {t("scan.single.zoomStep", { zoom: step })}
                  </span>
                </button>
              ))}
            </div>
          )}
        </div>

        <ResultSheet
          running={phase === "running"}
          recognised={status === "ok"}
          top={top}
          others={others}
          recent={recent}
          images={images}
        />
      </div>
    </div>
  )
}

function StartPanel({
  readiness,
  phase,
  stage,
  error,
  onStart,
}: {
  readonly readiness: ReturnType<typeof useScanReadiness>
  readonly phase: Phase
  readonly stage: ScanEngineStage | null
  readonly error: string
  readonly onStart: () => void
}) {
  const { t } = useTranslation()
  const needsDownload = readiness.status !== null && !readiness.ready
  return (
    <div className="absolute inset-0 flex flex-col items-center justify-center gap-3 bg-camera-cover px-8 text-center">
      <ScanLine className="size-10 text-camera-text-2" aria-hidden="true" />
      <div className="space-y-1">
        <p className="text-17 font-bold">{t("scan.single.title")}</p>
        <p className="text-13 text-camera-text-2">{t("scan.single.hint")}</p>
      </div>
      {needsDownload && !readiness.downloading && (
        <div className="flex flex-col items-center gap-2">
          <p className="text-13 text-camera-text-2">
            {t("scan.single.needModels", { mb: megabytes(readiness.missingBytes) })}
          </p>
          {readiness.status.manifest === "ok" && (
            <Button variant="primary" size="lg" onClick={readiness.download}>
              <Download className="size-5" aria-hidden="true" />
              {t("scan.single.download", { mb: megabytes(readiness.missingBytes) })}
            </Button>
          )}
          {readiness.failed && (
            <p role="alert" className="text-13 text-camera-error">
              {t("scan.single.downloadFailed")}
            </p>
          )}
          <Link to="/cards/scan/models" className="text-13 text-camera-text-2 underline">
            {t("scan.single.manage")}
          </Link>
        </div>
      )}
      {readiness.downloading && (
        <div className="flex w-full max-w-64 flex-col gap-1.5">
          {SCAN_PARTS.filter((part) => readiness.progress[part] !== undefined).map((part) => {
            const [got, total] = readiness.progress[part] ?? [0, 0]
            const pct = total ? Math.min(100, Math.round((got / total) * 100)) : 0
            return (
              <div key={part} className="flex flex-col gap-0.5">
                <div className="flex justify-between text-12 text-camera-text-2">
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
                  className="h-1.5 overflow-hidden rounded-pill bg-camera-track"
                >
                  <div
                    className="h-full bg-camera-text transition-[width]"
                    style={{ width: `${String(pct)}%` }}
                  />
                </div>
              </div>
            )
          })}
        </div>
      )}
      {readiness.ready && (
        <>
          <Button variant="primary" size="lg" onClick={onStart} disabled={phase === "starting"}>
            <Camera className="size-5" aria-hidden="true" />
            {phase === "starting" && stage
              ? t(`scan.single.stage.${stage}`)
              : t("scan.single.openCamera")}
          </Button>
          <p className="text-12 text-camera-text-2">{t("scan.single.local")}</p>
        </>
      )}
      {phase === "error" && (
        <p role="alert" className="text-13 text-camera-error">
          {t("scan.single.cantStart", { error })}
        </p>
      )}
    </div>
  )
}

function cardLink(result: ScanResult) {
  return result.summary
    ? { to: printingPath(result.summary), state: routeState(result.summary) }
    : { to: printingPath({ cardNo: result.card.no }) }
}

function ResultName({ result }: { readonly result: ScanResult }) {
  return <>{result.name?.primary.text ?? result.card.name}</>
}

function Thumb({
  result,
  images,
  className,
}: {
  readonly result: ScanResult
  readonly images: ImageIndex | undefined
  readonly className: string
}) {
  if (result.faceImage)
    return (
      <img
        src={result.faceImage}
        alt=""
        className={cn("aspect-[63/88] rounded-sm bg-surface-3 object-cover", className)}
      />
    )
  if (!result.summary || !result.name)
    return <span aria-hidden="true" className={cn("rounded-sm bg-surface-3", className)} />
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

/** The best card, then one row with the other guesses and cards seen a moment ago. */
function ResultSheet({
  running,
  recognised,
  top,
  others,
  recent,
  images,
}: {
  readonly running: boolean
  readonly recognised: boolean
  readonly top: ScanResult | undefined
  readonly others: readonly ScanResult[]
  readonly recent: readonly ScanResult[]
  readonly images: ImageIndex | undefined
}) {
  const { t } = useTranslation()
  // Positioned, so the images' screen-reader text stays inside the scrolling row.
  const chip = "relative flex shrink-0 items-center gap-1.5 rounded-pill py-0.5 pr-2.5 pl-0.5"
  return (
    <div className="flex shrink-0 flex-col gap-2 bg-surface-1 px-3 py-3 text-text-1">
      {running && top ? (
        <Link
          {...cardLink(top)}
          className={cn(
            "relative flex items-center gap-3 rounded-button border p-2",
            recognised ? "border-accent bg-accent-soft" : "border-border",
          )}
        >
          <Thumb result={top} images={images} className="w-15 shrink-0" />
          <div className="min-w-0 flex-1">
            <p className={cn("truncate text-16 font-bold", !recognised && "text-text-2")}>
              <ResultName result={top} />
            </p>
            <p className="truncate text-13 text-text-2">
              {t("scan.single.resultLine", {
                no: top.card.no,
                hint: recognised ? t("scan.single.tapForEffect") : t("scan.single.notSure"),
              })}
            </p>
          </div>
          <ChevronRight className="size-5 shrink-0 text-text-3" aria-hidden="true" />
        </Link>
      ) : (
        <div className="flex h-25 items-center justify-center rounded-button border border-dashed border-border text-13 text-text-3">
          {running ? t("scan.single.noCardYet") : t("scan.single.resultHere")}
        </div>
      )}
      {running && (others.length > 0 || recent.length > 0) && (
        <div className="relative flex h-8 min-w-0 [scrollbar-width:none] items-center gap-2 overflow-x-auto text-12 [&::-webkit-scrollbar]:hidden">
          {others.map((result) => (
            <Link
              key={result.card.cardId}
              {...cardLink(result)}
              className={cn(chip, "border border-border")}
            >
              <Thumb result={result} images={images} className="w-5" />
              <span className="max-w-28 truncate">
                <ResultName result={result} />
              </span>
            </Link>
          ))}
          {recent.length > 0 && (
            <span className="flex shrink-0 items-center gap-1 border-l border-border pl-2 text-text-3">
              <History className="size-3.5" aria-hidden="true" />
              {t("scan.single.recent")}
            </span>
          )}
          {recent.map((result) => (
            <Link
              key={result.card.cardId}
              {...cardLink(result)}
              className={cn(chip, "bg-surface-2 text-text-2")}
            >
              <Thumb result={result} images={images} className="w-5" />
              <span className="max-w-28 truncate">
                <ResultName result={result} />
              </span>
            </Link>
          ))}
        </div>
      )}
    </div>
  )
}
