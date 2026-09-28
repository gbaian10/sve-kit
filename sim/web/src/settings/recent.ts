export const RECENT_KEY = "sve-kit:recent"
export const RECENT_LIMIT = 8

// Stored data may come from another build; enforce the contract on read too, not only on push.
function read(storage: Storage | undefined): string[] {
  try {
    const raw = (storage ?? localStorage).getItem(RECENT_KEY)
    if (raw === null) return []
    const parsed: unknown = JSON.parse(raw)
    if (!Array.isArray(parsed)) return []
    const strings = parsed.filter((item): item is string => typeof item === "string")
    return [...new Set(strings)].slice(0, RECENT_LIMIT)
  } catch {
    return []
  }
}

export function loadRecent(storage?: Storage): string[] {
  return read(storage)
}

/** Newest first, no duplicates, capped; returns the new list even when storage is unavailable. */
export function pushRecent(id: string, storage?: Storage): string[] {
  const next = [id, ...read(storage).filter((item) => item !== id)].slice(0, RECENT_LIMIT)
  try {
    ;(storage ?? localStorage).setItem(RECENT_KEY, JSON.stringify(next))
  } catch {
    // Storage blocked: the caller still gets the list for this session.
  }
  return next
}
