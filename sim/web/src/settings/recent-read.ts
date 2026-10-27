export const RECENT_KEY = "sve-kit:recent"
export const RECENT_LIMIT = 8

/** Printing ids most recently opened, newest first; unknown ids are dropped when read against a snapshot. */
export type Recent = readonly string[]

export type StorageSource = Storage | (() => Storage) | undefined

export function resolveRecentStorage(source: StorageSource): Storage {
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
    const raw = resolveRecentStorage(source).getItem(RECENT_KEY)
    return raw === null ? [] : sanitize(JSON.parse(raw))
  } catch {
    return []
  }
}
