import { useSyncExternalStore } from "react"

// Whether the published card back failed to load, per address. Every back on screen shows the same
// file, so the first failure switches all of them to the drawn one at once.
const failed = new Set<string>()
const listeners = new Set<() => void>()

export function markCardBackFailed(url: string): void {
  if (failed.has(url)) return
  failed.add(url)
  for (const listener of listeners) listener()
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

export function useCardBackFailed(url: string): boolean {
  return useSyncExternalStore(subscribe, () => failed.has(url))
}
