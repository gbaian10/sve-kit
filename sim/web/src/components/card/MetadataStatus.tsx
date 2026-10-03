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
    if (snapshot && progress.persistent && !dataSaver) void client.prefetchImages()
    if (snapshot && progress.persistent && !dataSaver)
      return () => {
        client.cancelImagePrefetch()
      }
  }, [client, snapshot, dataSaver, progress.persistent])
  useEffect(
    () => () => {
      client.cancelImagePrefetch()
    },
    [client],
  )
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
