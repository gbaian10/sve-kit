import { useSyncExternalStore } from "react"

// Which published pictures (the card back, the counter chips) failed to load, by address. Every
// copy on screen shows the same file, so the first failure switches all of them to the drawn
// stand-in at once.
const failed = new Set<string>()
const listeners = new Set<() => void>()

export function markArtFailed(url: string): void {
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

export function useArtFailed(url: string): boolean {
  return useSyncExternalStore(subscribe, () => failed.has(url))
}
