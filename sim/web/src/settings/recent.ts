import { useSyncExternalStore } from "react"

import type { Recent } from "./recent-read"
import { createRecentStore } from "./recent-storage"

export type { Recent } from "./recent-read"
export { RECENT_KEY } from "./recent-read"

export const recentStore = createRecentStore()

export function useRecent(): Recent {
  return useSyncExternalStore(recentStore.subscribe, recentStore.get, recentStore.get)
}
