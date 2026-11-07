import { describe, expect, it } from "vitest"

import { scanPublication } from "../../test-utils/scan-publication"
import { parseScanManifest, ScanManifestError } from "./manifest"

type Mutable = Record<string, unknown>

function variant(change: (manifest: Mutable) => void): unknown {
  const manifest = structuredClone(scanPublication().manifest) as unknown as Mutable
  change(manifest)
  return manifest
}

function kindOf(value: unknown): string {
  try {
    parseScanManifest(value)
  } catch (error) {
    if (error instanceof ScanManifestError) return error.kind
    throw error
  }
  return "ok"
}

const part = (manifest: Mutable, key: string) => manifest[key] as Mutable
const jp = (manifest: Mutable) => part(part(manifest, "index"), "jp")

describe("scan manifest", () => {
  it("reads both models, their scoring and each edition's index", () => {
    const manifest = parseScanManifest(scanPublication().manifest)
    expect(manifest.det).toMatchObject({ version: 1, input: 256, stride: 8, grow: 1.6 })
    expect(manifest.rec).toMatchObject({
      version: 1,
      height: 312,
      width: 224,
      dim: 4,
      scoring: { hubWeight: 0.75, accept: 0.5, margin: 0.03 },
    })
    expect(manifest.index.jp).toMatchObject({ rec: 1, dataVersion: "2026-09-29", count: 5 })
    expect(manifest.index.en?.vectors.bytes).toBe(20)
  })

  it("accepts a list without an edition's index", () => {
    expect(parseScanManifest(scanPublication({ en: null }).manifest).index.en).toBeUndefined()
  })

  it("marks a list in another format as needing a newer site", () => {
    expect(kindOf(scanPublication({ format: 2 }).manifest)).toBe("unsupported")
    expect(kindOf({})).toBe("unsupported")
  })

  it.each([
    ["not an object", () => null],
    [
      "a file outside the scan folders",
      () => variant((m) => (part(m, "det")["file"] = "../det.onnx")),
    ],
    ["a malformed hash", () => variant((m) => (part(m, "rec")["sha256"] = "abc"))],
    ["a fractional size", () => variant((m) => (part(m, "rec")["bytes"] = 1.5))],
    [
      "a recogniser input that is not [height, width]",
      () => variant((m) => (part(m, "rec")["input"] = [312])),
    ],
    ["missing scoring", () => variant((m) => delete part(m, "rec")["scoring"])],
    ["a grid that does not divide the input", () => variant((m) => (part(m, "det")["stride"] = 7))],
    ["an index for another recogniser", () => variant((m) => (jp(m)["rec"] = 2))],
    ["an index whose sizes miss its card count", () => variant((m) => (jp(m)["count"] = 6))],
    ["an index without a data version", () => variant((m) => (jp(m)["data_version"] = ""))],
  ])("rejects %s as invalid", (_, make) => {
    expect(kindOf(make())).toBe("invalid")
  })
})
