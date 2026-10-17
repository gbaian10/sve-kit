import workerUrl from "./image-sw?worker&url"

export function registerImageCache(): void {
  if (import.meta.env.PROD && "serviceWorker" in navigator) {
    // Chromium's module script fetch omits cookies; Vite already bundles this as a classic script.
    void navigator.serviceWorker
      .register(workerUrl, { type: "classic", scope: "/", updateViaCache: "none" })
      .catch(() => undefined)
  }
}
