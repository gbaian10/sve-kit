import { Check, ChevronLeft, Download, RefreshCw, Trash2 } from "lucide-react"
import { type ReactNode, useEffect, useState } from "react"
import { useTranslation } from "react-i18next"
import { Link } from "react-router"

import { useRouteEdition } from "../../app/snapshot"
import { Button } from "../../components/ui/Button"
import { cn } from "../../components/ui/cn"
import { Notice } from "../../components/ui/Notice"
import {
  appScanModels,
  runtimePart,
  SCAN_BACKENDS,
  SCAN_PARTS,
  type ScanBackend,
  type ScanModelsStatus,
  type ScanModelStore,
  type ScanPart,
  type ScanPartStatus,
} from "../../data"
import { saveScanBackend } from "../../settings"
import { type ScanDownloadRun, scanDownloadsFor, useScanDownloads } from "./downloads"
import { closeScanEngine } from "./engine"
import { currentBackend, neededParts, webgpuAvailable } from "./runtime-choice"
import { megabytes } from "./scan-hooks"

const VERSION_PREFIX: Partial<Record<ScanPart, string>> = { det: "det-", rec: "rec-" }

export interface ScanModelsPageProps {
  readonly store?: ScanModelStore
}

/** What the card scanner keeps on this device: each part downloads, updates and deletes on its own. */
export function ScanModelsPage({ store = appScanModels }: ScanModelsPageProps) {
  const { t } = useTranslation()
  const downloads = scanDownloadsFor(store)
  const edition = useRouteEdition()
  const { runs, finished } = useScanDownloads(downloads)
  const [status, setStatus] = useState<ScanModelsStatus | null>(null)
  const [removing, setRemoving] = useState(false)
  const [changes, setChanges] = useState(0)
  const [backend, setBackend] = useState<ScanBackend | null>(null)
  const [gpuAvailable, setGpuAvailable] = useState(false)

  useEffect(() => {
    let active = true
    void store.status(edition).then((next) => {
      if (active) setStatus(next)
    })
    return () => {
      active = false
    }
  }, [store, edition, finished, changes])

  useEffect(() => {
    let active = true
    void currentBackend().then((next) => {
      if (active) setBackend(next)
    })
    void webgpuAvailable().then((available) => {
      if (active) setGpuAvailable(available)
    })
    return () => {
      active = false
    }
  }, [])

  // Only the chosen engine's files are needed; the other one downloads when the user switches to it.
  const needed: readonly ScanPart[] = backend ? neededParts(backend) : []
  const busy = removing || SCAN_PARTS.some((part) => runs[part]?.state === "running")
  const canDownload = status?.manifest === "ok"
  const pending = status
    ? needed.filter(
        (part) => status.parts[part].state !== "ready" && status.parts[part].missingBytes > 0,
      )
    : []
  const neededBytes = status
    ? pending.reduce((total, part) => total + status.parts[part].missingBytes, 0)
    : 0
  const allReady =
    status !== null &&
    backend !== null &&
    needed.every((part) => status.parts[part].state === "ready")

  const choose = (next: ScanBackend) => {
    if (next === backend) return
    saveScanBackend(next)
    setBackend(next)
    closeScanEngine()
  }

  const remove = async (parts: "all" | ScanPart) => {
    setRemoving(true)
    try {
      if (parts === "all") await store.removeAll()
      else await store.remove(parts)
      closeScanEngine()
    } finally {
      setRemoving(false)
      setChanges((n) => n + 1)
    }
  }

  return (
    <div className="mx-auto flex w-full max-w-2xl flex-col gap-4 pt-4 pb-6">
      <Link
        to="/cards/scan"
        className="flex items-center gap-1 self-start text-14 text-text-2 hover:text-text-1"
      >
        <ChevronLeft className="size-4" aria-hidden="true" /> {t("scan.models.back")}
      </Link>
      <div>
        <h1 className="text-24 font-bold">{t("scan.models.title")}</h1>
        <p className="mt-1 text-14 text-text-2">{t("scan.models.intro")}</p>
      </div>

      {status && status.manifest !== "ok" && (
        <Notice
          tone={status.manifest === "offline" ? "info" : "warning"}
          role={status.manifest === "offline" ? "status" : "alert"}
          actions={
            status.manifest === "unsupported" ? (
              <Button
                size="sm"
                onClick={() => {
                  window.location.reload()
                }}
              >
                <RefreshCw className="size-4" aria-hidden="true" /> {t("scan.models.list.reload")}
              </Button>
            ) : undefined
          }
        >
          {t(`scan.models.list.${status.manifest}`)}
        </Notice>
      )}

      <div className="flex flex-wrap items-center gap-3 rounded-button border border-border bg-surface-1 p-3">
        <div className="min-w-0 flex-1 basis-24 text-13">
          <p className="font-semibold text-text-1">
            {status === null
              ? "…"
              : allReady
                ? t("scan.models.allReady")
                : neededBytes > 0
                  ? t("scan.models.needed", { mb: megabytes(neededBytes) })
                  : t("scan.models.unavailable")}
          </p>
          {status && (
            <p className="text-text-3">
              {t("scan.models.stored", { mb: megabytes(status.storedBytes) })}
            </p>
          )}
        </div>
        <div className="flex shrink-0 gap-2">
          <Button
            size="sm"
            variant="primary"
            disabled={busy || !canDownload || pending.length === 0}
            onClick={() => void downloads.start(pending, edition)}
          >
            <Download className="size-4" aria-hidden="true" /> {t("scan.models.downloadAll")}
          </Button>
          <Button
            size="sm"
            variant="danger"
            disabled={busy || !status || status.storedBytes === 0}
            onClick={() => void remove("all")}
          >
            <Trash2 className="size-4" aria-hidden="true" /> {t("scan.models.removeAll")}
          </Button>
        </div>
      </div>

      <ul className="flex flex-col gap-3">
        {SCAN_PARTS.map((part) => {
          const engine = SCAN_BACKENDS.find((b) => runtimePart(b) === part)
          if (engine === undefined)
            return (
              <PartRow
                key={part}
                part={part}
                status={status?.parts[part]}
                run={runs[part]}
                busy={busy}
                canDownload={canDownload}
                edition={t(`options.edition.${edition}`)}
                onDownload={() => void downloads.start([part], edition)}
                onRemove={() => void remove(part)}
              />
            )
          // Both engines share one card, in the place of the first; it acts on the chosen one.
          if (engine !== SCAN_BACKENDS[0] || !backend) return null
          const chosen = runtimePart(backend)
          const other = SCAN_BACKENDS.find((b) => b !== backend) ?? backend
          const otherStatus = status?.parts[runtimePart(other)]
          return (
            <PartRow
              key="runtime"
              part={chosen}
              status={status?.parts[chosen]}
              run={runs[chosen]}
              busy={busy}
              canDownload
              edition={t(`options.edition.${edition}`)}
              heading={{
                name: t("scan.models.runtime.name"),
                desc: t("scan.models.runtime.desc"),
              }}
              chooser={
                <RuntimeChooser
                  backend={backend}
                  gpuAvailable={gpuAvailable}
                  parts={status?.parts}
                  busy={busy}
                  onChoose={choose}
                />
              }
              extra={
                otherStatus?.state === "ready" ? (
                  <p className="flex flex-wrap items-center gap-x-2 text-12 text-text-3">
                    {t("scan.models.runtime.other", {
                      engine: t(`scan.models.${runtimePart(other)}.name`),
                      mb: megabytes(otherStatus.bytes),
                    })}
                    <Button
                      size="sm"
                      variant="ghost"
                      className="text-danger"
                      disabled={busy}
                      onClick={() => void remove(runtimePart(other))}
                    >
                      <Trash2 className="size-4" aria-hidden="true" /> {t("scan.models.remove")}
                    </Button>
                  </p>
                ) : null
              }
              onDownload={() => void downloads.start([chosen], edition)}
              onRemove={() => void remove(chosen)}
            />
          )
        })}
      </ul>
      <p className="text-12 text-text-3">{t("scan.models.footer")}</p>
    </div>
  )
}

function PartRow({
  part,
  status,
  run,
  busy,
  canDownload,
  edition,
  heading,
  chooser,
  extra,
  onDownload,
  onRemove,
}: {
  readonly part: ScanPart
  readonly status: ScanPartStatus | undefined
  readonly run: ScanDownloadRun | undefined
  readonly busy: boolean
  readonly canDownload: boolean
  readonly edition: string
  /** The engine card names the engine in general rather than the chosen part. */
  readonly heading?: { readonly name: string; readonly desc: string }
  readonly chooser?: ReactNode
  readonly extra?: ReactNode
  readonly onDownload: () => void
  readonly onRemove: () => void
}) {
  const { t, i18n } = useTranslation()
  const running = run?.state === "running" ? run : undefined
  const pct = running
    ? Math.min(100, Math.round((running.got / Math.max(1, running.total)) * 100))
    : 0
  const state = status?.state
  const unpublished =
    part === "rec" && status?.state === "missing" && status.bytes === 0 && canDownload
  const prefix = VERSION_PREFIX[part]
  const titleId = `scan-part-${part}`
  return (
    <li
      aria-labelledby={titleId}
      className="flex flex-col gap-2 rounded-button border border-border bg-surface-1 p-3"
    >
      <div className="flex items-start gap-2">
        <div className="min-w-0 flex-1">
          <p id={titleId} className="text-16 font-bold">
            {heading?.name ?? t(`scan.models.${part}.name`)}
          </p>
          <p className="text-13 text-text-2">{heading?.desc ?? t(`scan.models.${part}.desc`)}</p>
          {unpublished && (
            <p className="mt-0.5 text-12 text-text-3">
              {t("scan.models.rec.noIndex", { edition })}
            </p>
          )}
        </div>
        <span
          className={cn(
            "inline-flex shrink-0 items-center gap-1 rounded-pill px-2 py-0.5 text-12 font-semibold",
            running
              ? "bg-info-soft text-info"
              : state === "ready"
                ? "bg-success-soft text-success"
                : state === "update"
                  ? "bg-warning-soft text-warning"
                  : "bg-surface-2 text-text-3",
          )}
        >
          {state === "ready" && !running && <Check className="size-3.5" aria-hidden="true" />}
          {running
            ? t("scan.models.state.downloading", { pct })
            : state === undefined
              ? "…"
              : t(`scan.models.state.${state}`)}
        </span>
      </div>

      {chooser}
      {extra}
      {prefix !== undefined &&
        status?.version !== undefined &&
        (status.indexBytes !== undefined &&
        status.cards !== undefined &&
        status.dataVersion !== undefined ? (
          // The index updates on its own with new sets, so its size and data version show apart from the model.
          <dl className="grid grid-cols-[1fr_auto] gap-x-3 text-13 text-text-2">
            <dt>{t("scan.models.rec.model", { version: `${prefix}${String(status.version)}` })}</dt>
            <dd className="text-text-3 tabular-nums">
              {t("scan.models.size", { mb: megabytes(status.bytes - status.indexBytes) })}
            </dd>
            <dt>{t("scan.models.rec.indexName", { edition })}</dt>
            <dd className="text-text-3 tabular-nums">
              {t("scan.models.size", { mb: megabytes(status.indexBytes) })}
            </dd>
            <dd className="col-span-2 text-12 text-text-3">
              {t("scan.models.rec.index", {
                cards: status.cards.toLocaleString(i18n.language),
                dataVersion: status.dataVersion,
              })}
            </dd>
          </dl>
        ) : (
          <p className="text-13 text-text-2">
            {t("scan.models.version", { version: `${prefix}${String(status.version)}` })}
          </p>
        ))}

      {running && (
        <div
          role="progressbar"
          aria-label={t(`scan.models.${part}.name`)}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={pct}
          className="h-1.5 overflow-hidden rounded-pill bg-surface-3"
        >
          <div className="h-full bg-info transition-[width]" style={{ width: `${String(pct)}%` }} />
        </div>
      )}

      {run?.state === "failed" && (
        <Notice tone="danger" role="alert">
          {t(`scan.models.error.${run.error}`)}
        </Notice>
      )}

      <div className="flex items-center justify-between gap-2">
        <span className="text-13 text-text-3 tabular-nums">
          {status && status.bytes > 0 ? t("scan.models.size", { mb: megabytes(status.bytes) }) : ""}
        </span>
        <div className="flex gap-2">
          {(state === "ready" || state === "update") && (
            <Button
              size="sm"
              variant="ghost"
              className="text-danger"
              disabled={busy}
              onClick={onRemove}
            >
              <Trash2 className="size-4" aria-hidden="true" /> {t("scan.models.remove")}
            </Button>
          )}
          {state !== "ready" && !unpublished && (
            <Button size="sm" disabled={busy || !canDownload} onClick={onDownload}>
              <Download className="size-4" aria-hidden="true" />{" "}
              {state === "update" ? t("scan.models.update") : t("scan.models.download")}
            </Button>
          )}
        </div>
      </div>
    </li>
  )
}

/** The engine choice: one tile per engine, the chosen one marked; only the chosen one downloads. */
function RuntimeChooser({
  backend,
  gpuAvailable,
  parts,
  busy,
  onChoose,
}: {
  readonly backend: ScanBackend
  readonly gpuAvailable: boolean
  readonly parts: Readonly<Record<ScanPart, ScanPartStatus>> | undefined
  readonly busy: boolean
  readonly onChoose: (backend: ScanBackend) => void
}) {
  const { t } = useTranslation()
  return (
    <div
      role="radiogroup"
      aria-label={t("scan.models.runtime.name")}
      className="grid grid-cols-2 gap-2"
    >
      {SCAN_BACKENDS.map((engine) => {
        const part = runtimePart(engine)
        const on = engine === backend
        const unsupported = engine === "webgpu" && !gpuAvailable
        const bytes = parts?.[part].bytes ?? 0
        return (
          <button
            key={engine}
            type="button"
            role="radio"
            aria-checked={on}
            disabled={busy || unsupported}
            onClick={() => {
              onChoose(engine)
            }}
            className={cn(
              "flex min-w-0 flex-col items-start gap-0.5 rounded-button border p-2.5 text-left",
              on
                ? "border-accent bg-accent-soft ring-1 ring-accent ring-inset"
                : "border-border bg-surface-2 hover:bg-surface-3",
              unsupported && "opacity-50",
            )}
          >
            <span className="flex items-center gap-1.5 text-14 font-bold">
              {on && <Check className="size-4 text-accent" aria-hidden="true" />}
              {t(`scan.models.${part}.name`)}
            </span>
            <span className="text-12 text-text-2">
              {unsupported ? t("scan.models.runtime.unsupported") : t(`scan.models.${part}.desc`)}
            </span>
            {bytes > 0 && (
              <span className="text-12 text-text-3 tabular-nums">
                {t("scan.models.size", { mb: megabytes(bytes) })}
                {parts?.[part].state === "ready" && ` · ${t("scan.models.state.ready")}`}
              </span>
            )}
          </button>
        )
      })}
    </div>
  )
}
