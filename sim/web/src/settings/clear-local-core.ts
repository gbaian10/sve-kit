import { PREFS_KEY } from "./prefs"
import { RECENT_KEY } from "./recent"

function deleteDatabase(name: string): Promise<void> {
  return new Promise((resolve) => {
    const request = indexedDB.deleteDatabase(name)
    // Blocked means another tab still has it open; it goes once that tab lets go, so do not wait.
    request.onsuccess =
      request.onerror =
      request.onblocked =
        () => {
          resolve()
        }
  })
}

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
      indexedDB
        .databases()
        .then((databases) =>
          Promise.all(databases.flatMap(({ name }) => (name ? [deleteDatabase(name)] : []))),
        ),
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
