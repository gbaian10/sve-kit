import { useSyncExternalStore } from "react"

import {
  appScanModels,
  SCAN_PARTS,
  ScanModelError,
  type ScanModelErrorKind,
  type ScanModelStore,
  type ScanPart,
} from "../../data"
import type { Region } from "../../domain/search"
import { closeScanEngine } from "./engine"

export type ScanDownloadRun =
  | { readonly state: "running"; readonly got: number; readonly total: number }
  | { readonly state: "failed"; readonly error: ScanModelErrorKind | "unknown" }

export interface ScanDownloadState {
  readonly runs: Partial<Readonly<Record<ScanPart, ScanDownloadRun>>>
  /** Counts finished downloads, so screens know to read the device status again. */
  readonly finished: number
}

/**
 * Model downloads outlive the screen that started them: a user may leave the models page while a
 * part downloads and see its progress on the scan screen.
 */
function createScanDownloads(store: ScanModelStore) {
  let state: ScanDownloadState = { runs: {}, finished: 0 }
  const listeners = new Set<() => void>()
  const set = (part: ScanPart, run: ScanDownloadRun | undefined, finished = state.finished) => {
    const runs: Partial<Record<ScanPart, ScanDownloadRun>> = {}
    for (const key of SCAN_PARTS) {
      const value = key === part ? run : state.runs[key]
      if (value) runs[key] = value
    }
    state = { runs, finished }
    for (const listener of listeners) listener()
  }

  async function one(part: ScanPart, edition: Region): Promise<void> {
    if (state.runs[part]?.state === "running") return
    set(part, { state: "running", got: 0, total: 0 })
    try {
      await store.download(part, edition, (got, total) => {
        set(part, { state: "running", got, total })
      })
      closeScanEngine()
      set(part, undefined, state.finished + 1)
    } catch (error) {
      set(part, {
        state: "failed",
        error: error instanceof ScanModelError ? error.kind : "unknown",
      })
    }
  }

  return {
    get: () => state,
    subscribe: (listener: () => void) => {
      listeners.add(listener)
      return () => {
        listeners.delete(listener)
      }
    },
    /** Downloads the parts side by side; each part reports and fails on its own. */
    start: (parts: readonly ScanPart[], edition: Region): Promise<void> =>
      Promise.all(parts.map((part) => one(part, edition))).then(() => undefined),
  }
}

export type ScanDownloads = ReturnType<typeof createScanDownloads>

const managers = new WeakMap<ScanModelStore, ScanDownloads>()

/** The one download manager of a model store; the app's store is the default. */
export function scanDownloadsFor(store: ScanModelStore = appScanModels): ScanDownloads {
  let downloads = managers.get(store)
  if (!downloads) {
    downloads = createScanDownloads(store)
    managers.set(store, downloads)
  }
  return downloads
}

export function useScanDownloads(downloads: ScanDownloads): ScanDownloadState {
  return useSyncExternalStore(downloads.subscribe, downloads.get, downloads.get)
}
