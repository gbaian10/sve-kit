import { useSyncExternalStore } from "react"

import { createSnapshotClient, type SnapshotClient, type SnapshotStatus } from "./client"

export type SnapshotRoot = "cdn" | "preview"

const CDN_BASE = import.meta.env.VITE_CDN_BASE ?? "/cdn"
const PREVIEW_BASE = "/cdn-preview"

const clients: Record<SnapshotRoot, SnapshotClient> = {
  cdn: createSnapshotClient(CDN_BASE),
  preview: createSnapshotClient(PREVIEW_BASE, { entry: "preview" }),
}

// The preview root is a dev-only switch, kept in memory on purpose: nothing persists it.
let active: SnapshotRoot = "cdn"
const listeners = new Set<() => void>()

export function snapshotClient(root: SnapshotRoot = active): SnapshotClient {
  return clients[root]
}

export function activeSnapshotRoot(): SnapshotRoot {
  return active
}

export function setActiveSnapshotRoot(root: SnapshotRoot): void {
  active = root
  for (const listener of listeners) listener()
}

function subscribeRoot(listener: () => void): () => void {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

export function useSnapshotRoot(): SnapshotRoot {
  return useSyncExternalStore(subscribeRoot, activeSnapshotRoot, activeSnapshotRoot)
}

export function useSnapshotStatus(client: SnapshotClient): SnapshotStatus {
  return useSyncExternalStore(client.subscribe, client.status, client.status)
}
