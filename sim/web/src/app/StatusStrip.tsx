import { useTranslation } from "react-i18next"

import { Button } from "../components/ui/Button"
import { Notice } from "../components/ui/Notice"
import type { SearchReadiness } from "./snapshot"

// The one place a page says what the data layer is doing: downloading (which step, and how many
// startup files have arrived), updating behind a usable generation, or failed with a retry.
// Ready and quiet renders nothing, so a refresh served from the verified cache shows no strip.
export function StatusStrip({ readiness }: { readonly readiness: SearchReadiness }) {
  const { t } = useTranslation()
  if (readiness.phase === "error" || (readiness.phase === "ready" && readiness.error)) {
    return (
      <Notice
        tone="danger"
        role="alert"
        actions={
          <Button className="h-9" onClick={readiness.retry}>
            {t("status.retry")}
          </Button>
        }
      >
        <span className="font-semibold">
          {readiness.phase === "error" ? t("status.loadFailed") : t("status.updateFailed")}
        </span>{" "}
        <span className="text-text-2">{t("status.loadFailedHint")}</span>
      </Notice>
    )
  }
  if (readiness.phase === "downloading") {
    const loading = readiness.loading
    const files =
      loading?.step === "bootstrap" && loading.total !== undefined && loading.total > 0
        ? { done: loading.done ?? 0, total: loading.total }
        : null
    // The entry and manifest steps are a version check; only the startup files take seconds.
    const downloading = loading?.step === "bootstrap"
    return (
      <Notice tone="info" role="status">
        <span className="flex flex-wrap items-center gap-x-2 gap-y-1">
          <span>
            {downloading
              ? t("status.downloadingFiles")
              : t(`status.step.${loading?.step ?? "index"}`)}
          </span>
          {files && (
            <span aria-hidden="true" className="tabular-nums">
              {files.done}/{files.total}
            </span>
          )}
        </span>
        {files && (
          <span
            role="progressbar"
            aria-label={t("status.downloadProgress")}
            aria-valuemin={0}
            aria-valuemax={files.total}
            aria-valuenow={files.done}
            className="mt-1.5 block h-1.5 w-full max-w-80 overflow-hidden rounded-full bg-surface-2"
          >
            <span
              className="block h-full rounded-full bg-accent"
              style={{ width: `${String((files.done / files.total) * 100)}%` }}
            />
          </span>
        )}
      </Notice>
    )
  }
  if (readiness.updating) {
    const step = readiness.loading?.step ?? "manifest"
    return (
      <Notice tone="info" role="status">
        {t("status.updating", { step: t(`status.step.${step}`) })}
      </Notice>
    )
  }
  return null
}
