import { useMediaQuery } from "./useMediaQuery"

/** Whether the OS currently prefers dark; the CSS follows it on its own, this is for UI state. */
export function useSystemDark(): boolean {
  return useMediaQuery("(prefers-color-scheme: dark)")
}
