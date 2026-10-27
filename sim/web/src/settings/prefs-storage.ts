import type { Prefs } from "./prefs"
import { readPrefs, type StorageSource, writePrefs } from "./prefs-read-write"

export interface PrefsStore {
  readonly get: () => Prefs
  /** Applies the patch in memory first, so the UI follows even when persisting fails. */
  readonly set: (patch: Partial<Prefs>) => boolean
  readonly subscribe: (listener: () => void) => () => void
  /** Re-reads storage (another tab wrote it, or a test cleared it) and notifies subscribers. */
  readonly reload: () => void
}

export function createPrefsStore(source?: StorageSource): PrefsStore {
  let current = readPrefs(source)
  const listeners = new Set<() => void>()
  return {
    get: () => current,
    set: (patch) => {
      current = { ...current, ...patch }
      for (const listener of listeners) listener()
      return writePrefs(current, source)
    },
    subscribe: (listener) => {
      listeners.add(listener)
      return () => {
        listeners.delete(listener)
      }
    },
    reload: () => {
      current = readPrefs(source)
      for (const listener of listeners) listener()
    },
  }
}
