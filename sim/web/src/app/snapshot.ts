import { useEffect } from "react"

import {
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
