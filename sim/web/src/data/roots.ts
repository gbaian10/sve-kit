import { useSyncExternalStore } from "react"

import { appBackgroundDownloads } from "./background-downloads"
import { createSnapshotClient, type SnapshotClient, type SnapshotStatus } from "./client"

export type SnapshotRoot = "cdn" | "preview"

const CDN_BASE = import.meta.env.VITE_CDN_BASE ?? "/cdn"
const PREVIEW_BASE = "/cdn-preview"

const bases: Record<SnapshotRoot, string> = { cdn: CDN_BASE, preview: PREVIEW_BASE }

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

/** URL prefix of a snapshot root; other published files (the card scanner's) sit under it too. */
export function snapshotBase(root: SnapshotRoot = active): string {
  return bases[root]
}

export function activeSnapshotRoot(): SnapshotRoot {
  return active
}

export function setActiveSnapshotRoot(root: SnapshotRoot): void {
  if (active !== root) appBackgroundDownloads.adopt(null)
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
