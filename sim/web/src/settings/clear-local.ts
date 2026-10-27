import { clearManagedCaches } from "../data"
import { clearLocalData } from "./clear-local-core"

const COLD_START_KEY = "sve-kit:cold-start"
let cacheClearResult: "cleared" | "failed" | undefined

export function getCacheClearResult() {
  return cacheClearResult
}

/**
 * Asks the next page load to start cold. Clearing from the running page could race downloads that
 * write caches again before the reload, so the clearing happens at the next start instead.
 */
export function requestColdStart(scope: "all" | "caches" = "all"): void {
  try {
    localStorage.setItem(COLD_START_KEY, scope)
  } catch {
    if (scope === "caches") throw new Error("Cannot request cache clearing")
  }
}

/** Runs a requested cold start; call before anything downloads, caches or registers a worker. */
export async function runRequestedColdStart(): Promise<void> {
  try {
    const scope = localStorage.getItem(COLD_START_KEY)
    if (scope === null) return
    if (scope === "caches") {
      try {
        await clearManagedCaches()
        cacheClearResult = "cleared"
      } catch {
        cacheClearResult = "failed"
      }
      localStorage.removeItem(COLD_START_KEY)
      return
    }
  } catch {
    return
  }
  await clearLocalData()
}
