import { Check, ChevronLeft, Download, RefreshCw, Trash2 } from "lucide-react"
import { useEffect, useState } from "react"
import { useTranslation } from "react-i18next"
import { Link } from "react-router"

import { useRouteEdition } from "../../app/snapshot"
import { Button } from "../../components/ui/Button"
import { cn } from "../../components/ui/cn"
import { Notice } from "../../components/ui/Notice"
import {
  appScanModels,
  SCAN_PARTS,
  type ScanModelsStatus,
  type ScanModelStore,
  type ScanPart,
  type ScanPartStatus,
} from "../../data"
import { type ScanDownloadRun, scanDownloadsFor, useScanDownloads } from "./downloads"
import { closeScanEngine } from "./engine"

const megabytes = (bytes: number) => (bytes / 1048576).toFixed(1)

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

  useEffect(() => {
    let active = true
    void store.status(edition).then((next) => {
      if (active) setStatus(next)
    })
    return () => {
      active = false
    }
  }, [store, edition, finished, changes])

  const busy = removing || SCAN_PARTS.some((part) => runs[part]?.state === "running")
  const canDownload = status?.manifest === "ok"
  const pending = status
    ? SCAN_PARTS.filter(
        (part) => status.parts[part].state !== "ready" && status.parts[part].missingBytes > 0,
      )
    : []
  const neededBytes = status
    ? pending.reduce((total, part) => total + status.parts[part].missingBytes, 0)
    : 0
  const allReady =
    status !== null && SCAN_PARTS.every((part) => status.parts[part].state === "ready")

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
        to="/cards"
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
        {SCAN_PARTS.map((part) => (
          <PartRow
            key={part}
            part={part}
            status={status?.parts[part]}
            run={runs[part]}
            busy={busy}
            canDownload={part === "runtime" || canDownload}
            edition={t(`options.edition.${edition}`)}
            onDownload={() => void downloads.start([part], edition)}
            onRemove={() => void remove(part)}
          />
        ))}
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
  onDownload,
  onRemove,
}: {
  readonly part: ScanPart
  readonly status: ScanPartStatus | undefined
  readonly run: ScanDownloadRun | undefined
  readonly busy: boolean
  readonly canDownload: boolean
  readonly edition: string
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
            {t(`scan.models.${part}.name`)}
          </p>
          <p className="text-13 text-text-2">{t(`scan.models.${part}.desc`)}</p>
          {part === "rec" && status?.cards !== undefined && status.dataVersion !== undefined && (
            <p className="mt-0.5 text-12 text-text-3">
              {t("scan.models.rec.index", {
                edition,
                cards: status.cards.toLocaleString(i18n.language),
                dataVersion: status.dataVersion,
              })}
            </p>
          )}
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

      {prefix !== undefined && status?.version !== undefined && (
        <p className="text-13 text-text-2">
          {t("scan.models.version", { version: `${prefix}${String(status.version)}` })}
        </p>
      )}

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
