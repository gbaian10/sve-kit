import { useEffect, useSyncExternalStore } from "react"
import { useTranslation } from "react-i18next"

import type { SnapshotClient } from "../../data"
import { Button } from "../ui/Button"

/** This progress refers to metadata bytes, never to all picture blobs or verified offline text. */
export function MetadataStatus({ client }: { readonly client: SnapshotClient }) {
  const { t } = useTranslation()
  const progress = useSyncExternalStore(
    client.subscribe,
    client.metadataStatus,
    client.metadataStatus,
  )
  const snapshot = client.snapshot()
  useEffect(() => {
    if (snapshot && progress.persistent) void client.prefetchImages()
    if (snapshot && progress.persistent)
      return () => {
        client.cancelImagePrefetch()
      }
  }, [client, snapshot, progress.persistent])
  useEffect(
    () => () => {
      client.cancelImagePrefetch()
    },
    [client],
  )
  // Progress is only news while it is happening; a finished download leaves the list alone.
  if (progress.state === "complete" && progress.persistent) return null
  return (
    <div className="flex flex-wrap items-center gap-2 text-13 text-text-2">
      <span role="status">
        {t(`metadata.${progress.state}`, { done: progress.done, total: progress.total })}
      </span>
      {!progress.persistent && !progress.checking && <span>{t("metadata.degraded")}</span>}
      {progress.state === "running" ? (
        <Button
          onClick={() => {
            client.cancelImagePrefetch()
          }}
        >
          {t("metadata.cancel")}
        </Button>
      ) : progress.persistent && progress.state !== "complete" ? (
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
