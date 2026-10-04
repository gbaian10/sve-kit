// @vitest-environment node
import { describe, expect, it } from "vitest"

import {
  canonicalText,
  type JsonObject,
  parseStrict,
  stringValue,
} from "../../src/data/format-v1/json"
import { readSnapshot } from "../../src/data/format-v1/reader"
import { hex } from "../../src/data/format-v1/sha256"
import { buildSnapshot, canonicalSize } from "./build"
import { CARDS } from "./cards"

// Deterministic stand-in for the WebP encoder: the bytes only need to be unique per seed and size.
const stubImage = ({ width, height, seed }: { width: number; height: number; seed: number }) =>
  Promise.resolve(
    new TextEncoder().encode(`stub-webp:${String(width)}x${String(height)}:${String(seed)}`),
  )

function payloadsOf(snapshot: Awaited<ReturnType<typeof buildSnapshot>>): Map<string, Uint8Array> {
  const payloads = new Map<string, Uint8Array>()
  for (const file of snapshot.manifest["files"] as JsonObject[]) {
    const bytes = snapshot.files.get(stringValue(file["path"]))
    if (!bytes) throw new Error(`missing blob ${stringValue(file["path"])}`)
    payloads.set(stringValue(file["key"]), bytes)
  }
  return payloads
}

describe("fixture snapshot", async () => {
  const snapshot = await buildSnapshot({ encodeImage: stubImage })

  it("is accepted by the reader with every card present", () => {
    const view = readSnapshot(snapshot.manifest, payloadsOf(snapshot))
    expect(view["card"]).toHaveLength(CARDS.length)
    expect(view["printing"]?.length).toBe(CARDS.reduce((n, card) => n + card.printings.length, 0))
    expect(view["face_revision"]?.length).toBeGreaterThan(CARDS.length)
    expect(snapshot.counts["card"]).toBe(CARDS.length)
  })

  it("prints the wording each printing actually carried, not the latest revision", () => {
    const view = readSnapshot(snapshot.manifest, payloadsOf(snapshot))
    const revisions = new Map(
      (view["face_revision"] ?? []).map((row) => [stringValue(row["id"]), row]),
    )
    const printings = new Map((view["printing"] ?? []).map((row) => [stringValue(row["id"]), row]))
    const currentOf = (face: JsonObject | undefined, region: string) =>
      (face?.["current"] as JsonObject[]).find((entry) => entry["region"] === region)?.[
        "revision_id"
      ]
    const printedEffect = (printingId: string) =>
      ((printings.get(printingId)?.["faces"] as JsonObject[])[0] as JsonObject)[
        "printed_effect_unit_id"
      ]
    // Effective erratum: the listed printing keeps revision 1, the current revision is 2.
    expect(printedEffect("p:bp01-020")).toBe(revisions.get("r:f:bp01-020:jp:1")?.["effect_unit_id"])
    expect(printedEffect("p:bp01-020")).not.toBe(
      revisions.get("r:f:bp01-020:jp:2")?.["effect_unit_id"],
    )
    const face020 = (view["face"] ?? []).find((row) => row["id"] === "f:bp01-020")
    expect(currentOf(face020, "jp")).toBe("r:f:bp01-020:jp:2")
    // Announced but not yet effective: printed text and current text are both revision 1.
    expect(printedEffect("p:bp01-021")).toBe(revisions.get("r:f:bp01-021:jp:1")?.["effect_unit_id"])
    const face021 = (view["face"] ?? []).find((row) => row["id"] === "f:bp01-021")
    expect(currentOf(face021, "jp")).toBe("r:f:bp01-021:jp:1")
    expect(view["route_override"]).toEqual([{ route_key: "BP01-002", printing_id: "p:bp01-002" }])
    expect((snapshot.manifest["source_windows"] as JsonObject[]).map((row) => row["kind"])).toEqual(
      ["errata", "qa"],
    )
  })

  it("is deterministic", async () => {
    const again = await buildSnapshot({ encodeImage: stubImage })
    expect([...again.files.keys()].sort()).toEqual([...snapshot.files.keys()].sort())
    for (const [path, bytes] of snapshot.files)
      expect(hex(again.files.get(path) ?? new Uint8Array())).toBe(hex(bytes))
  })

  it("publishes the current snapshot through index format 2", () => {
    const index = parseStrict(
      snapshot.files.get("snapshots/versions/index.json") ?? "",
    ) as JsonObject
    expect(index["index_format"]).toBe(2)
    expect(index["previous"]).toBeNull()
    expect((index["current"] as JsonObject)["manifest_path"]).toBe(snapshot.manifestPath)
    expect(canonicalText(parseStrict(snapshot.files.get(snapshot.manifestPath) ?? ""))).toBe(
      canonicalText(snapshot.manifest),
    )
  })

  it("stays well under the 1 MiB fixture budget without images", () => {
    expect(canonicalSize(snapshot)).toBeLessThan(1024 * 1024)
  })
})
