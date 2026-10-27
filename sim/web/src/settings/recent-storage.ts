import {
  readRecent,
  type Recent,
  RECENT_KEY,
  RECENT_LIMIT,
  resolveRecentStorage,
  type StorageSource,
} from "./recent-read"

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
      resolveRecentStorage(source).setItem(RECENT_KEY, JSON.stringify(next))
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
