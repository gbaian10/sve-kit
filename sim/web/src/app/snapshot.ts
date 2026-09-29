import { useEffect, useState } from "react"

import {
  type Catalog,
  catalogOf,
  type ImageIndex,
  imageIndexOf,
  type LoadedSnapshot,
  type SnapshotClient,
  snapshotClient,
  type SnapshotRoot,
  type SnapshotStatus,
  useSnapshotRoot,
  useSnapshotStatus,
} from "../data"

/** The snapshot every page reads: loading starts with the shell and follows the active root. */
export function useActiveSnapshot(): {
  readonly client: SnapshotClient
  readonly status: SnapshotStatus
  readonly root: SnapshotRoot
} {
  const root = useSnapshotRoot()
  const client = snapshotClient(root)
  const status = useSnapshotStatus(client)
  useEffect(() => {
    void client.load()
  }, [client])
  return { client, status, root }
}

/** The catalog of the loaded snapshot, or null while nothing is loaded yet. */
export function useCatalog(): {
  readonly client: SnapshotClient
  readonly status: SnapshotStatus
  readonly catalog: Catalog | null
} {
  const { client, status } = useActiveSnapshot()
  const snapshot = client.snapshot()
  return { client, status, catalog: snapshot ? catalogOf(snapshot) : null }
}

/** The image index once its file has arrived; undefined before that (cells show text cards). */
export function useImageIndex(client: SnapshotClient, ready: boolean): ImageIndex | undefined {
  const [loaded, setLoaded] = useState<{
    readonly snapshot: LoadedSnapshot
    readonly index: ImageIndex
  }>()
  const snapshot = client.snapshot()
  useEffect(() => {
    if (!ready || !snapshot) return
    let cancelled = false
    imageIndexOf(client).then(
      (index) => {
        if (!cancelled) setLoaded({ snapshot, index })
      },
      () => {
        // Cards stay as text cards; the snapshot status surface reports download problems.
      },
    )
    return () => {
      cancelled = true
    }
  }, [client, ready, snapshot])
  // A swapped snapshot shows text cards until its own image file has arrived, never the old images.
  return loaded !== undefined && loaded.snapshot === snapshot ? loaded.index : undefined
}
