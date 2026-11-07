// Which ONNX Runtime build the card scanner uses on this device. Per device only: the models are the
// same either way, and the choice decides which runtime file is downloaded and used.

type ScanBackendChoice = "webgpu" | "cpu"

const KEY = "sve-kit:scan-backend"
const CHOICES: readonly ScanBackendChoice[] = ["webgpu", "cpu"]

/** The saved choice, if any. Storage can be blocked, so every access is guarded. */
export function savedScanBackend(): ScanBackendChoice | undefined {
  try {
    const raw = localStorage.getItem(KEY)
    return CHOICES.find((choice) => choice === raw)
  } catch {
    return undefined
  }
}

export function saveScanBackend(choice: ScanBackendChoice): void {
  try {
    localStorage.setItem(KEY, choice)
  } catch {
    // Without storage the choice reverts to detection on the next visit.
  }
}
