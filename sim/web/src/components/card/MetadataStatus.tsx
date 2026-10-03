import { useEffect, useSyncExternalStore } from "react"
import { useTranslation } from "react-i18next"

import type { SnapshotClient } from "../../data"
import { usePrefs } from "../../settings"
import { Button } from "../ui/Button"

/** This progress refers to metadata bytes, never to all picture blobs or verified offline text. */
export function MetadataStatus({ client }: { readonly client: SnapshotClient }) {
  const { t } = useTranslation()
  const { dataSaver } = usePrefs()
  const progress = useSyncExternalStore(
    client.subscribe,
    client.metadataStatus,
    client.metadataStatus,
  )
  const snapshot = client.snapshot()
  useEffect(() => {
    if (snapshot && !dataSaver) void client.prefetchImages()
    return () => {
      client.cancelImagePrefetch()
    }
  }, [client, snapshot, dataSaver])
  return (
    <div className="flex flex-wrap items-center gap-2 text-13 text-text-2">
      <span role="status">
        {t(`metadata.${progress.state}`, { done: progress.done, total: progress.total })}
      </span>
      {!progress.persistent && <span>{t("metadata.degraded")}</span>}
      {progress.state === "running" ? (
        <Button
          onClick={() => {
            client.cancelImagePrefetch()
          }}
        >
          {t("metadata.cancel")}
        </Button>
      ) : progress.state !== "complete" ? (
        <Button
          onClick={() => {
            void client.prefetchImages()
          }}
        >
          {t("metadata.retry")}
        </Button>
      ) : null}
    </div>
  )
}
