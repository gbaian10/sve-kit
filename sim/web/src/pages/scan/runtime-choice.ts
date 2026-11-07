import { runtimePart, type ScanBackend, type ScanPart } from "../../data"
import { savedScanBackend } from "../../settings"

/** WebGPU when the browser can actually hand out an adapter; otherwise the CPU build. */
export async function webgpuAvailable(): Promise<boolean> {
  try {
    const adapter = await navigator.gpu.requestAdapter()
    return adapter !== null
  } catch {
    // No navigator.gpu at all, or the adapter request failed: the CPU build always works.
    return false
  }
}

/** The backend to use now: the saved choice, else what this browser supports. */
export async function currentBackend(): Promise<ScanBackend> {
  return savedScanBackend() ?? ((await webgpuAvailable()) ? "webgpu" : "cpu")
}

/** The parts a scan with this backend needs: its runtime, the finder and the recogniser. */
export function neededParts(backend: ScanBackend): readonly ScanPart[] {
  return [runtimePart(backend), "det", "rec"]
}
