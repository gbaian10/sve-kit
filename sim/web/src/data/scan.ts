import runtimeUrl from "onnxruntime-web/ort-wasm-simd-threaded.wasm?url"

import { snapshotBase } from "./roots"
import { createScanModelStore } from "./scan-models"

/** The app's scanner files: published next to the active snapshot root, cached per device. */
export const appScanModels = createScanModelStore({
  caches: () => caches,
  fetcher: (url, init) => fetch(url, init),
  scanBase: () => `${snapshotBase()}/scan`,
  runtime: { url: runtimeUrl, bytes: Number(import.meta.env.SVE_SCAN_RUNTIME_BYTES) },
})
