import { describe, expect, it } from "vitest"

import { checkConstruction } from "../domain/deck/construction"
import { constructionIndexUrl, decodeConstructionIndex } from "./construction-index"
import type { JsonObject } from "./format-v3/json"

const HASH = `sha256:${"ab".repeat(32)}`

// The wire shape of docs/schema/export/construction-index.md for a handful of cards.
const wire = (): JsonObject => ({
  construction_index_format: 1,
  region: "jp",
  snapshot: { data_version: "preview-20261103T000000Z-0001", manifest_sha256: HASH },
  classes: ["elf", "neutral", "royal"],
  titles: ["cardfight_vanguard", "umamusume_pretty_derby"],
  title_rules: { cardfight_vanguard: "vanguard" },
  zones: ["none", "leader", "main", "evolve"],
  names: 5,
  cards: {
    id: ["c:amulet", "c:carrot", "c:cut", "c:ep", "c:leader"],
    name: [0, 1, 2, 3, 4],
    class: [0, 1, 1, -1, 0],
    titles: [1, 2, 0, 0, 0],
    zone: [2, 3, 2, 0, 1],
  },
  copies: [[1, "evolve", 10]],
  singleton_except: [[2, [2]]],
  start_amulet: [0],
  formats: {
    standard: {
      coverage: [["2026-10-26", "2026-10-27"]],
      restrictions: [
        {
          id: "r:1",
          kind: "copy_limit",
          from: "2025-10-01",
          until: null,
          max: 0,
          max_groups: null,
          members: [[0, "main", 0]],
        },
      ],
    },
  },
})

describe("decodeConstructionIndex", () => {
  it("decodes dictionary codes, bit masks and per-card rules", () => {
    const index = decodeConstructionIndex(wire(), HASH, "jp")
    expect(index.card("c:amulet")).toEqual({
      name: 0,
      classCode: "elf",
      titles: ["cardfight_vanguard"],
      zone: "main",
      startAmulet: true,
    })
    expect(index.card("c:carrot")).toMatchObject({
      classCode: "neutral",
      titles: ["umamusume_pretty_derby"],
      zone: "evolve",
      copies: { evolve: 10 },
    })
    expect(index.card("c:cut")?.singletonExcept).toEqual(new Set([2]))
    expect(index.card("c:ep")).toMatchObject({ classCode: null, zone: "none" })
    expect(index.card("c:missing")).toBeUndefined()
    expect(index.titleRules.get("cardfight_vanguard")).toBe("vanguard")
    expect(index.formats.get("standard")?.restrictions[0]).toEqual({
      id: "r:1",
      kind: "copy_limit",
      from: "2025-10-01",
      until: null,
      max: 0,
      maxGroups: null,
      members: [{ name: 0, scope: "main", option: 0 }],
    })
  })

  it("feeds the validator directly", () => {
    const index = decodeConstructionIndex(wire(), HASH, "jp")
    const result = checkConstruction(
      { basis: { kind: "class" }, leader: "c:leader", main: [["c:amulet", 1]], evolve: [] },
      index,
      { format: "standard", date: "2026-10-26" },
    )
    expect(result.violations.map((violation) => violation.code).sort()).toEqual([
      "banned",
      "mainTooFew",
    ])
  })

  it.each([
    ["another snapshot", (value: JsonObject) => ({ ...value, snapshot: { manifest_sha256: "x" } })],
    ["another region", (value: JsonObject) => ({ ...value, region: "en" })],
    ["a newer format", (value: JsonObject) => ({ ...value, construction_index_format: 2 })],
    ["an unknown rules name", (value: JsonObject) => ({ ...value, singleton_except: [[2, [9]]] })],
    [
      "uneven card columns",
      (value: JsonObject) => ({
        ...value,
        cards: { ...(value["cards"] as JsonObject), zone: [2] },
      }),
    ],
  ])("rejects %s", (_name, mutate) => {
    expect(() => decodeConstructionIndex(mutate(wire()), HASH, "jp")).toThrow()
  })

  it("locates the index by the snapshot's manifest hash", () => {
    expect(constructionIndexUrl("/cdn", HASH, "en")).toBe(
      `/cdn/construction/${"ab".repeat(32)}/en.json`,
    )
  })
})
