import { useTranslation } from "react-i18next"

import {
  activeSnapshotRoot,
  setActiveSnapshotRoot,
  type SnapshotClient,
  useSnapshotRoot,
  useSnapshotStatus,
} from "../data"
import { formatDataVersion } from "../domain/dataVersion"

// The badge is 28px tall, so it has its own compact button instead of the 44px design buttons.
const BUTTON =
  "h-7 shrink-0 rounded-button px-2 text-12 font-semibold whitespace-nowrap text-text-1 hover:bg-surface-2"

// Dev only: which root is active, how far loading got, and a way to flip roots or retry without
// touching the settings UI. Production builds never render it.
export function DevBadge({ client }: { readonly client: SnapshotClient }) {
  const { t } = useTranslation()
  const root = useSnapshotRoot()
  const status = useSnapshotStatus(client)
  const text =
    status.state === "ready"
      ? status.updating !== undefined
        ? t("dev.status.updating", {
            version: formatDataVersion(status.dataVersion),
            phase: t(`dev.phase.${status.updating}`),
          })
        : status.updateError
          ? t("dev.status.updateFailed", {
              version: formatDataVersion(status.dataVersion),
              kind: t(`dev.error.${status.updateError.kind}`),
            })
          : formatDataVersion(status.dataVersion)
      : status.state === "loading"
        ? t("dev.status.loading", { phase: t(`dev.phase.${status.phase}`) })
        : status.state === "error"
          ? t("dev.status.error", { kind: t(`dev.error.${status.kind}`) })
          : t("dev.status.idle")
  const canRetry =
    status.state === "error" || (status.state === "ready" && status.updateError !== undefined)
  return (
    <div
      role="status"
      title={status.state === "ready" ? status.dataVersion : undefined}
      className="fixed right-2 bottom-24 z-40 flex max-w-[calc(100vw-1rem)] items-center gap-2 rounded-pill border border-border bg-surface-1 py-1 pr-1 pl-3 text-12 text-text-2 shadow-lg lg:bottom-3 phone-landscape:bottom-3"
    >
      <span className="shrink-0 font-semibold">{t(`dev.root.${root}`)}</span>
      <span className="min-w-0 truncate">{text}</span>
      {canRetry && (
        <button
          type="button"
          className={BUTTON}
          onClick={() => void (status.state === "error" ? client.retry() : client.reload())}
        >
          {t("dev.retry")}
        </button>
      )}
      {(import.meta.env.SVE_PREVIEW_CONFIGURED === "1" || root === "preview") && (
        <button
          type="button"
          className={BUTTON}
          onClick={() => {
            setActiveSnapshotRoot(activeSnapshotRoot() === "cdn" ? "preview" : "cdn")
          }}
        >
          {root === "cdn" ? t("dev.usePreview") : t("dev.useCdn")}
        </button>
      )}
    </div>
  )
}
