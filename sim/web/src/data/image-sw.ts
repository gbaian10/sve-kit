import { cachedImage, versionedImage } from "./image-cache"

interface ImageFetchEvent {
  readonly request: Request
  respondWith: (response: Promise<Response>) => void
}
interface ImageWorker {
  readonly caches: CacheStorage
  readonly clients: { claim: () => Promise<void> }
  skipWaiting: () => Promise<void>
  addEventListener: {
    (name: "fetch", callback: (event: ImageFetchEvent) => void): void
    (
      name: "activate" | "install",
      callback: (event: { waitUntil: (promise: Promise<void>) => void }) => void,
    ): void
  }
}
const worker = self as unknown as ImageWorker
worker.addEventListener("install", (event) => {
  event.waitUntil(worker.skipWaiting())
})
worker.addEventListener("activate", (event) => {
  event.waitUntil(worker.clients.claim())
})
worker.addEventListener("fetch", (event) => {
  if (event.request.method === "GET" && versionedImage(event.request.url))
    event.respondWith(cachedImage(event.request, worker.caches, fetch))
})
