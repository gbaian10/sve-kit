import { useSyncExternalStore } from "react"

export const RECENT_KEY = "sve-kit:recent"
const RECENT_LIMIT = 8

/** Printing ids most recently opened, newest first; unknown ids are dropped when read against a snapshot. */
export type Recent = readonly string[]

type StorageSource = Storage | (() => Storage) | undefined

function resolveStorage(source: StorageSource): Storage {
  if (source === undefined) return localStorage
  return typeof source === "function" ? source() : source
}

function sanitize(raw: unknown): Recent {
  if (!Array.isArray(raw)) return []
  const ids = raw.filter((value): value is string => typeof value === "string" && value !== "")
  return [...new Set(ids)].slice(0, RECENT_LIMIT)
}

export function readRecent(source?: StorageSource): Recent {
  try {
    const raw = resolveStorage(source).getItem(RECENT_KEY)
    return raw === null ? [] : sanitize(JSON.parse(raw))
  } catch {
    return []
  }
}

export interface RecentStore {
  readonly get: () => Recent
  /** Moves the id to the front and trims to the limit; persisting may fail silently. */
  readonly push: (printingId: string) => void
  readonly clear: () => void
  readonly subscribe: (listener: () => void) => () => void
}

export function createRecentStore(source?: StorageSource): RecentStore {
  let current = readRecent(source)
  const listeners = new Set<() => void>()
  const write = (next: Recent) => {
    current = next
    for (const listener of listeners) listener()
    try {
      resolveStorage(source).setItem(RECENT_KEY, JSON.stringify(next))
    } catch {
      // In-memory state already moved on; storage is a convenience.
    }
  }
  return {
    get: () => current,
    push: (printingId) => {
      write([printingId, ...current.filter((id) => id !== printingId)].slice(0, RECENT_LIMIT))
    },
    clear: () => {
      write([])
    },
    subscribe: (listener) => {
      listeners.add(listener)
      return () => {
        listeners.delete(listener)
      }
    },
  }
}

export const recentStore: RecentStore = createRecentStore()

export function useRecent(): Recent {
  return useSyncExternalStore(recentStore.subscribe, recentStore.get, recentStore.get)
}
