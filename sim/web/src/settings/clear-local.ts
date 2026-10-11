import { PREFS_KEY } from "./prefs"
import { RECENT_KEY } from "./recent"

/**
 * Drops everything this origin stored (verified snapshots, search index, image caches, service
 * workers) so the next load is a true cold load; display settings and recent searches stay.
 */
export async function clearLocalData(): Promise<void> {
  const keep = new Set([PREFS_KEY, RECENT_KEY])
  const tasks: Promise<unknown>[] = []
  if (typeof caches !== "undefined")
    tasks.push(caches.keys().then((names) => Promise.all(names.map((name) => caches.delete(name)))))
  if (typeof indexedDB !== "undefined" && "databases" in indexedDB)
    tasks.push(
      indexedDB.databases().then((databases) => {
        databases.forEach((database) => {
          if (database.name) indexedDB.deleteDatabase(database.name)
        })
      }),
    )
  if ("serviceWorker" in navigator)
    tasks.push(
      navigator.serviceWorker
        .getRegistrations()
        .then((registrations) => Promise.all(registrations.map((item) => item.unregister()))),
    )
  await Promise.allSettled(tasks)
  for (const key of Object.keys(localStorage)) if (!keep.has(key)) localStorage.removeItem(key)
  sessionStorage.clear()
}
