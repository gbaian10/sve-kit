// @vitest-environment node
import { describe, expect, it } from "vitest"

import { arrayValue, objectValue, stringValue } from "./format-v3/json"
import { isCompatible, readContainer, readManifest } from "./format-v3/reader"
import { v3Fixture } from "./v3-fixture"

describe("DSL program snapshot profile", () => {
  it("admits 4.1 only with its declared reader version and required DSL capability", () => {
    const previous = objectValue(v3Fixture("manifest.json"))
    const current = {
      ...previous,
      format_version: "4.1.0",
      min_reader_version: "4.1.0",
      required_capabilities: [
        ...arrayValue(previous["required_capabilities"] ?? null),
        "dsl-programs-v1",
      ].sort(),
    }
    expect(isCompatible(previous)).toBe(true)
    expect(isCompatible(current)).toBe(true)
    expect(readManifest(current).manifest).toEqual(current)
    expect(
      isCompatible({ ...current, required_capabilities: previous["required_capabilities"] ?? [] }),
    ).toBe(false)
    expect(isCompatible({ ...current, min_reader_version: "4.1.1" })).toBe(false)
    expect(isCompatible({ ...current, format_version: "4.2.0" })).toBe(false)
  })
})

describe("DSL profile tuple decoding", () => {
  it("decodes the shared engine rows using the 4.1 profile", () => {
    const native = objectValue(v3Fixture("mentioned-by-native.json"))
    const manifest = objectValue(native["manifest"])
    const file = arrayValue(manifest["files"])
      .map((item) => objectValue(item))
      .find((item) => item["key"] === "engine/shared/family-family/0")
    if (!file) throw new Error("missing synthetic engine file")
    const value = objectValue(objectValue(native["payloads"])[stringValue(file["key"])])
    const fragments = readContainer(file, { ...value, format_version: "4.1.0" }, "4.1.0")
    expect(fragments.find((fragment) => fragment.table === "card_mentioned_by")?.rows).toEqual([
      { card_id: "old_card", region: "jp", from_card_id: "card" },
    ])
  })
})
