import workerUrl from "./image-sw?worker&url"

export function registerImageCache(): void {
  if (import.meta.env.PROD && "serviceWorker" in navigator) {
    // The image worker is emitted at the root so every app route can use the same cache.
    void navigator.serviceWorker
      .register(workerUrl, { type: "module", scope: "/", updateViaCache: "none" })
      .catch(() => undefined)
  }
}
