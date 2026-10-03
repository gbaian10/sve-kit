import { useSyncExternalStore } from "react"
import { useTranslation } from "react-i18next"

import { Button } from "../components/ui/Button"
import type { SnapshotClient, SnapshotStatus } from "../data"

function subscribe(listener: () => void): () => void {
  window.addEventListener("online", listener)
  window.addEventListener("offline", listener)
  return () => {
    window.removeEventListener("online", listener)
    window.removeEventListener("offline", listener)
  }
}

/** A local active or compatible previous snapshot is usable, but cannot promise current pictures. */
export function SnapshotNotice({
  client,
  status,
}: {
  readonly client: SnapshotClient
  readonly status: SnapshotStatus
}) {
  const { t } = useTranslation()
  const online = useSyncExternalStore(
    subscribe,
    () => navigator.onLine,
    () => true,
  )
  if (status.state !== "ready" || (!status.outdated && !status.updateError && online)) return null
  return (
    <div
      role="status"
      className="flex flex-wrap items-center gap-2 border-b border-border bg-surface-2 px-4 py-2 text-13 text-text-2"
    >
      <span>{online ? t("snapshot.stale") : t("snapshot.offline")}</span>
      {status.updateError?.kind === "incompatible" && <span>{t("snapshot.updateApp")}</span>}
      {online && (
        <Button
          onClick={() => {
            void client.reload()
          }}
        >
          {t("snapshot.retry")}
        </Button>
      )}
    </div>
  )
}
