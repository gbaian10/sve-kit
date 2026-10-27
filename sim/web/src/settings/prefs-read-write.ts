import { DEFAULT_PREFS, type Prefs, PREFS_KEY, PREFS_VERSION, sanitize } from "./prefs"

export type StorageSource = Storage | (() => Storage) | undefined

// Even reading `localStorage` can throw (opaque origins, blocked site data), so resolve it inside try.
function resolveStorage(source: StorageSource): Storage {
  if (source === undefined) return localStorage
  return typeof source === "function" ? source() : source
}

// Trust only this build's version, as theme-boot.js does; DEFAULT_PREFS excludes removed fields.
/** Never throws; every field falls back to its default on its own. */
export function readPrefs(source?: StorageSource): Prefs {
  try {
    const storage = resolveStorage(source)
    const raw = storage.getItem(PREFS_KEY)
    if (raw !== null) return sanitize(JSON.parse(raw))
    return DEFAULT_PREFS
  } catch {
    return DEFAULT_PREFS
  }
}

export function writePrefs(prefs: Prefs, source?: StorageSource): boolean {
  try {
    resolveStorage(source).setItem(PREFS_KEY, JSON.stringify({ v: PREFS_VERSION, ...prefs }))
    return true
  } catch {
    return false
  }
}
