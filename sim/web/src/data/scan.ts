import { snapshotBase } from "./roots"
import { createScanModelStore } from "./scan-models"
import { scanRuntimeBinaries } from "./scan-runtime"

/** The app's scanner files: published next to the active snapshot root, cached per device. */
export const appScanModels = createScanModelStore({
  caches: () => caches,
  fetcher: (url, init) => fetch(url, init),
  scanBase: () => `${snapshotBase()}/scan`,
  runtimes: scanRuntimeBinaries(),
})

/** A printing face's small card image, named as the scan index says (`<n>`, other faces `<n>-f<face>`). */
export function scanCardImage(imageId: number, face: number): string {
  const name = face === 0 ? String(imageId) : `${String(imageId)}-f${String(face)}`
  return `${snapshotBase()}/images/card_s/${name}.webp`
}

/** Downloads a printing face's small card image (fetched, so its pixels can be read back). */
export async function loadScanCardImage(imageId: number, face: number): Promise<Blob> {
  const url = scanCardImage(imageId, face)
  const response = await fetch(url)
  if (!response.ok) throw new Error(`card image ${url}: ${String(response.status)}`)
  return response.blob()
}
