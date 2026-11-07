// The card scanner's ONNX Runtime binaries come from a public npm CDN, at the exact onnxruntime-web
// version this build's JavaScript was bundled from. The version and each binary's size and SHA-256
// are read from node_modules at build time (vite.config.ts), so the site never trusts the CDN's copy.

export type ScanBackend = "webgpu" | "cpu"
export const SCAN_BACKENDS: readonly ScanBackend[] = ["webgpu", "cpu"]

export interface ScanRuntimeBinary {
  /** File name in onnxruntime-web's dist folder; also the last path segment on the CDN. */
  readonly file: string
  readonly bytes: number
  readonly sha256: string
  /** Tried in order; the next one is used only when a download cannot be made at all. */
  readonly urls: readonly string[]
}

interface RuntimePins {
  readonly version: string
  readonly cpu: { readonly file: string; readonly bytes: number; readonly sha256: string }
  readonly webgpu: { readonly file: string; readonly bytes: number; readonly sha256: string }
}

const CDNS = [
  (version: string, file: string) =>
    `https://cdn.jsdelivr.net/npm/onnxruntime-web@${version}/dist/${file}`,
  (version: string, file: string) => `https://unpkg.com/onnxruntime-web@${version}/dist/${file}`,
]

export function scanRuntimeBinaries(
  pins: string = import.meta.env.SVE_SCAN_RUNTIME_PINS,
): Record<ScanBackend, ScanRuntimeBinary> {
  const parsed = JSON.parse(pins) as RuntimePins
  const binary = (row: RuntimePins["cpu"]): ScanRuntimeBinary => ({
    ...row,
    urls: CDNS.map((url) => url(parsed.version, row.file)),
  })
  return { cpu: binary(parsed.cpu), webgpu: binary(parsed.webgpu) }
}
