// @vitest-environment node
import { describe, expect, it } from "vitest"

import { v2Fixture } from "../v2-fixture"
import { READER_ERROR_CODES, SnapshotError } from "./errors"
import { canonical, canonicalText, type JsonObject, objectValue, stringValue } from "./json"
import { isCompatible, readSnapshot, verifyManifest } from "./reader"
import { validate } from "./schema"

function wire() {
  const manifest = objectValue(v2Fixture("manifest.json"))
  const files = manifest["files"] as JsonObject[]
  const payloads = new Map(
    files.map((file) => [
      stringValue(file["key"]),
      canonical(v2Fixture(`payloads/${stringValue(file["sha256"]).slice(7)}.json`)),
    ]),
  )
  return { manifest, files, payloads }
}

describe("current snapshot transport", () => {
  it("keeps distinct rejection codes and the independent golden view", () => {
    expect(new Set(READER_ERROR_CODES).size).toBe(READER_ERROR_CODES.length)
    const { manifest, payloads } = wire()
    expect(canonicalText(readSnapshot(manifest, payloads))).toBe(
      canonicalText(v2Fixture("expected-logical.json")),
    )
  })
  it.each(["1.0.0", "1.1.0", "3.0.0"])(
    "refuses unsupported format %s without reading payloads",
    (version) => {
      const { manifest, payloads } = wire()
      manifest["format_version"] = version
      expect(isCompatible(manifest)).toBe(false)
      expect(() => readSnapshot(manifest, payloads)).toThrow("unsupported-version")
      expect(() => {
        validate("Manifest", manifest, [], version)
      }).toThrow("unsupported-version")
    },
  )
  it.each(["2.0.0"])("accepts a compatible minimum reader version %s", (version) => {
    const { manifest, payloads } = wire()
    manifest["min_reader_version"] = version
    expect(canonicalText(readSnapshot(manifest, payloads))).toBe(
      canonicalText(v2Fixture("expected-logical.json")),
    )
  })
  it("refuses unsupported reader requirements or capabilities", () => {
    for (const field of ["min_reader_version", "required_capabilities"]) {
      const { manifest } = wire()
      manifest[field] = field === "min_reader_version" ? "3.0.0" : ["unknown"]
      expect(() => verifyManifest(manifest)).toThrow(/unsupported-version|schema/)
    }
  })
  it("rejects missing programs and an incomplete payload set", () => {
    const { manifest, payloads } = wire()
    payloads.delete("programs")
    expect(() => readSnapshot(manifest, payloads)).toThrow("payload-set")
    manifest["files"] = (manifest["files"] as JsonObject[]).filter(
      (file) => file["role"] !== "programs",
    )
    expect(() => readSnapshot(manifest, payloads)).toThrow("config-programs-count")
  })
  it("rejects cyclic dependencies before requesting payloads", () => {
    const { manifest, files } = wire()
    const config = files.find((file) => file["role"] === "config")
    const programs = files.find((file) => file["role"] === "programs")
    if (!config || !programs) throw new Error("missing fixture")
    config["dependencies"] = [{ key: programs["key"] ?? null, sha256: programs["sha256"] ?? null }]
    programs["dependencies"] = [{ key: config["key"] ?? null, sha256: config["sha256"] ?? null }]
    expect(() => verifyManifest(manifest)).toThrow("dependency-cycle")
  })
  it("rejects changed or reformatted payload bytes", () => {
    for (const extra of [false, true]) {
      const { manifest, payloads } = wire()
      const original = payloads.get("programs") ?? new Uint8Array()
      payloads.set("programs", extra ? new Uint8Array([...original, 10]) : canonical({}))
      expect(() => readSnapshot(manifest, payloads)).toThrow("blob-integrity")
    }
  })
  it("rejects an unsorted file list or a mismatched dependency hash", () => {
    const { manifest, files } = wire()
    manifest["files"] = [...files].reverse()
    expect(() => verifyManifest(manifest)).toThrow("files-unsorted")
    manifest["files"] = files
    const file = files.find((file) => (file["dependencies"] as JsonObject[]).length > 0)
    if (!file) throw new Error("missing fixture")
    const dependency = (file["dependencies"] as JsonObject[])[0]
    if (!dependency) throw new Error("missing dependency")
    dependency["sha256"] = `sha256:${"0".repeat(64)}`
    expect(() => verifyManifest(manifest)).toThrow(SnapshotError)
    expect(() => verifyManifest(manifest)).toThrow("dependency-hash")
  })
})
