import { useSyncExternalStore } from "react"

/** Whether a CSS media query currently matches; false before hydration and without a window. */
export function useMediaQuery(query: string): boolean {
  return useSyncExternalStore(
    (listener) => {
      const media = window.matchMedia(query)
      media.addEventListener("change", listener)
      return () => {
        media.removeEventListener("change", listener)
      }
    },
    () => window.matchMedia(query).matches,
    () => false,
  )
}
