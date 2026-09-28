import { useSyncExternalStore } from "react"

const QUERY = "(prefers-color-scheme: dark)"

function subscribe(listener: () => void): () => void {
  const media = window.matchMedia(QUERY)
  media.addEventListener("change", listener)
  return () => {
    media.removeEventListener("change", listener)
  }
}

function snapshot(): boolean {
  return window.matchMedia(QUERY).matches
}

/** Whether the OS currently prefers dark; the CSS follows it on its own, this is for UI state. */
export function useSystemDark(): boolean {
  return useSyncExternalStore(subscribe, snapshot, () => false)
}
