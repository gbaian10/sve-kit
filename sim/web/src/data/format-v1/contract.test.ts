// @vitest-environment node
import { describe, expect, it } from "vitest"

import { READER_ERROR_CODES, type ReaderErrorCode, SnapshotError } from "./errors"
import { canonical, canonicalText, type JsonObject, type JsonValue, parseStrict } from "./json"
import { readSnapshot, readTextAll } from "./reader"
import { digest } from "./sha256"

// Shared handwritten golden (docs/schema/snapshot-contract.md): the same files the Python reader
// reads, never regenerated from either reader.
const files = import.meta.glob<string>(
  "../../../../../tests/fixtures/snapshot-contract/v1/**/*.json",
  { query: "?raw", import: "default", eager: true },
)

// Formatted review copies are parsed leniently; the reader sees their canonical bytes.
function fixture(name: string): JsonValue {
  const key = Object.keys(files).find((path) => path.endsWith(`/v1/${name}`))
  if (!key) throw new Error(`fixture ${name} missing`)
  return JSON.parse(files[key] ?? "") as JsonValue
}
const clone = (value: JsonValue): JsonValue => JSON.parse(JSON.stringify(value)) as JsonValue
const manifest = () => clone(fixture("manifest.json")) as JsonObject
const expected = () => canonicalText(fixture("expected-logical.json"))

function payloads(): Map<string, Uint8Array> {
  const result = new Map<string, Uint8Array>()
  for (const name of ["bootstrap", "config", "detail", "history", "images", "programs"]) {
    result.set(name, canonical(fixture(`payloads/${name}.json`)))
  }
  return result
}

function fileOf(manifestValue: JsonObject, key: string): JsonObject {
  const entry = (manifestValue["files"] as JsonObject[]).find((file) => file["key"] === key)
  if (!entry) throw new Error(`file ${key} missing`)
  return entry
}

function replace(
  value: JsonValue,
  path: readonly (string | number)[],
  replacement: JsonValue,
): void {
  let current = value as JsonObject | JsonValue[]
  for (const segment of path.slice(0, -1)) {
    current = (current as Record<string, JsonValue>)[String(segment)] as JsonObject | JsonValue[]
  }
  ;(current as Record<string, JsonValue>)[String(path.at(-1))] = replacement
}

/** The fixture's mutation procedure: reseal one payload so byte checks pass and joins are reached. */
function reseal(manifestValue: JsonObject, key: string, data: Uint8Array): void {
  const file = fileOf(manifestValue, key)
  const hashed = digest(data)
  file["sha256"] = hashed
  file["bytes"] = data.length
  file["path"] = `snapshots/blobs/${hashed.slice(7)}.json`
  if (key === "detail" || key === "history") {
    for (const ref of (manifestValue["text_all"] as JsonObject)["contains"] as JsonObject[]) {
      if (ref["key"] === key) ref["sha256"] = hashed
    }
  }
  if (key === "detail") {
    const decoded = parseStrict(data) as JsonObject
    for (const count of file["row_counts"] as JsonObject[]) {
      const fragments = (decoded["tables"] as JsonObject)[count["table"] as string] as JsonObject[]
      count["count"] = ((fragments[0] as JsonObject)["rows"] as JsonValue[]).length
    }
  }
}

function codeOf(run: () => void): ReaderErrorCode {
  try {
    run()
  } catch (error) {
    if (error instanceof SnapshotError) return error.code
    throw error
  }
  throw new Error("accepted")
}

// Every shared counterexample must fail for its intended reason, not by accident elsewhere.
const EXPECTED_CODES: Record<string, readonly ReaderErrorCode[]> = {
  "dependency-hash": ["dependency-hash"],
  "base-hash": ["base-dependency"],
  "row-index-out-of-range": ["row-index"],
  "row-index-duplicate": ["rows-unsorted-or-duplicate"],
  "row-index-missing": ["row-index"],
  "face-ordinal-out-of-range": ["face-ordinal"],
  "face-ordinal-duplicate": ["face-ordinal"],
  "face-ordinal-missing": ["face-ordinal"],
  "dangling-text": ["dangling-reference"],
  "duplicate-translation": ["translation-duplicate"],
  "wrong-translation-partition": ["translation-partition"],
  "wrong-descriptor": ["schema"],
  "wrong-columns": ["schema"],
  "row-count": ["row-counts-mismatch"],
  "wrong-capability": ["unsupported-version"],
  "wrong-bucket-profile": ["schema"],
  "future-reader": ["unsupported-version"],
  "missing-base-dependency": ["base-dependency"],
  "nonempty-programs": ["schema"],
  "malformed-text-id": ["text-id-mismatch"],
  "current-in-history": ["primary-key-duplicate"],
  // The fixture swaps the sections array, so what it exercises is a dangling text_unit reference;
  // the vocabulary rule itself is covered by "does not repair a dangling vocabulary reference".
  "dangling-vocabulary": ["dangling-reference"],
  "spelling-undeclared": ["spelling-undeclared-parameter"],
  "spelling-disabled-uint": ["spelling-disabled-domain"],
  "spelling-disabled-variable": ["spelling-disabled-domain"],
  "parameter-inverted": ["parameter-range-inverted"],
  "parameter-duplicate": ["rows-unsorted-or-duplicate"],
  "parameter-unsorted": ["rows-unsorted-or-duplicate"],
  "hint-parameters-disagree": ["hints-parameters-differ"],
  "variant-unapproved": ["image-variant-unapproved"],
  "variant-unavailable": ["image-variant-unapproved"],
  "published-hour-25": ["schema"],
  "published-february-30": ["schema"],
  "revision-date-unicode": ["schema"],
}

describe("snapshot contract golden", () => {
  it("names every rejection with a distinct fixed code", () => {
    expect(new Set(READER_ERROR_CODES).size).toBe(READER_ERROR_CODES.length)
    for (const codes of Object.values(EXPECTED_CODES)) {
      for (const code of codes) expect(READER_ERROR_CODES).toContain(code)
    }
  })

  it("joins the golden snapshot into exactly the independent logical view", () => {
    expect(canonicalText(readSnapshot(manifest(), payloads()))).toBe(expected())
  })

  it("reads text_all into the same logical view as individual downloads", () => {
    const blobs = payloads()
    const attachments = new Map([
      ["images", blobs.get("images") ?? new Uint8Array()],
      ["programs", blobs.get("programs") ?? new Uint8Array()],
    ])
    const view = readTextAll(manifest(), canonical(fixture("text-all.json")), attachments)
    expect(canonicalText(view)).toBe(expected())
  })

  it("accepts an older minimum reader version", () => {
    const m = manifest()
    m["min_reader_version"] = "0.9.0"
    expect(canonicalText(readSnapshot(m, payloads()))).toBe(expected())
  })

  it("rejects every shared reader counterexample for its intended reason", () => {
    const cases = fixture("reader-invalid.json") as JsonObject[]
    expect(cases.length).toBe(34)
    for (const item of cases) {
      const name = item["name"] as string
      const m = manifest()
      const blobs = payloads()
      const target = item["target"] as string
      const value = target === "manifest" ? m : parseStrict(blobs.get(target) ?? new Uint8Array())
      replace(value, item["path"] as (string | number)[], item["value"] ?? null)
      if (target !== "manifest") {
        blobs.set(target, canonical(value))
        if (item["rehash"] === true) reseal(m, target, blobs.get(target) ?? new Uint8Array())
      }
      const code = codeOf(() => readSnapshot(m, blobs))
      const allowed = EXPECTED_CODES[name]
      expect(allowed, `no expectation for ${name}`).toBeDefined()
      expect(allowed, `${name}: got ${code}`).toContain(code)
    }
  })

  it("rejects a manifest without the programs file", () => {
    const m = manifest()
    m["files"] = (m["files"] as JsonObject[]).filter((file) => file["role"] !== "programs")
    const blobs = payloads()
    blobs.delete("programs")
    expect(codeOf(() => readSnapshot(m, blobs))).toBe("config-programs-count")
  })

  it("rejects cyclic dependencies", () => {
    const m = manifest()
    for (const [key, target] of [
      ["config", "programs"],
      ["programs", "config"],
    ] as const) {
      fileOf(m, key)["dependencies"] = [
        { key: target, sha256: fileOf(m, target)["sha256"] ?? null },
      ]
    }
    expect(codeOf(() => readSnapshot(m, payloads()))).toBe("dependency-cycle")
  })

  it("rejects reformatted or changed payload bytes", () => {
    const original = payloads().get("programs") ?? new Uint8Array()
    for (const data of [new TextEncoder().encode("{}"), new Uint8Array([...original, 0x0a])]) {
      const blobs = payloads()
      blobs.set("programs", data)
      expect(codeOf(() => readSnapshot(manifest(), blobs))).toBe("blob-integrity")
    }
  })

  function replaceBootstrap(m: JsonObject, blobs: Map<string, Uint8Array>, value: JsonValue): void {
    blobs.set("bootstrap", canonical(value))
    reseal(m, "bootstrap", blobs.get("bootstrap") ?? new Uint8Array())
    const hashed = fileOf(m, "bootstrap")["sha256"] ?? null
    fileOf(m, "detail")["dependencies"] = [{ key: "bootstrap", sha256: hashed }]
    const detail = parseStrict(blobs.get("detail") ?? new Uint8Array()) as JsonObject
    for (const table of ["printing", "face_revision"]) {
      const fragment = ((detail["tables"] as JsonObject)[table] as JsonObject[])[0] as JsonObject
      ;((fragment["base"] as JsonObject)["file"] as JsonObject)["sha256"] = hashed
    }
    for (const ref of (m["text_all"] as JsonObject)["contains"] as JsonObject[]) {
      if (ref["key"] === "bootstrap") ref["sha256"] = hashed
    }
    blobs.set("detail", canonical(detail))
    reseal(m, "detail", blobs.get("detail") ?? new Uint8Array())
  }

  it("requires a sorted base even when every hash is valid", () => {
    const m = manifest()
    const blobs = payloads()
    const boot = parseStrict(blobs.get("bootstrap") ?? new Uint8Array()) as JsonObject
    const fragment = ((boot["tables"] as JsonObject)["printing"] as JsonObject[])[0] as JsonObject
    fragment["rows"] = [...(fragment["rows"] as JsonValue[])].reverse()
    replaceBootstrap(m, blobs, boot)
    expect(codeOf(() => readSnapshot(m, blobs))).toBe("rows-unsorted-or-duplicate")
  })

  it("does not repair a dangling vocabulary reference", () => {
    const m = manifest()
    const blobs = payloads()
    const boot = parseStrict(blobs.get("bootstrap") ?? new Uint8Array()) as JsonObject
    const fragment = (
      (boot["tables"] as JsonObject)["face_revision"] as JsonObject[]
    )[0] as JsonObject
    ;((fragment["rows"] as JsonValue[][])[0] as JsonValue[])[5] = "missing_type"
    replaceBootstrap(m, blobs, boot)
    expect(codeOf(() => readSnapshot(m, blobs))).toBe("vocabulary-missing")
  })

  it("does not let text_all silently replace member content", () => {
    const m = manifest()
    const union = fixture("text-all.json") as JsonObject
    const member = (union["members"] as JsonObject[])[0] as JsonObject
    ;(member["payload"] as JsonObject)["format_version"] = "2.0.0"
    const data = canonical(union)
    const description = m["text_all"] as JsonObject
    const hashed = digest(data)
    description["sha256"] = hashed
    description["bytes"] = data.length
    description["path"] = `snapshots/blobs/${hashed.slice(7)}.json`
    const blobs = payloads()
    const attachments = new Map([
      ["images", blobs.get("images") ?? new Uint8Array()],
      ["programs", blobs.get("programs") ?? new Uint8Array()],
    ])
    expect(codeOf(() => readTextAll(m, data, attachments))).toBe("schema")
  })
})
