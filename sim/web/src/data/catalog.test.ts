// @vitest-environment node
import { describe, expect, it } from "vitest"

import { buildSnapshot } from "../../scripts/fixture/build"
import { DEFAULT_QUERY } from "../domain/query/model"
import { catalogOf, createCatalog } from "./catalog"
import type { Fetcher } from "./cdn"
import { createSnapshotClient } from "./client"

const stubImage = ({ width, height, seed }: { width: number; height: number; seed: number }) =>
  Promise.resolve(
    new TextEncoder().encode(`stub:${String(width)}x${String(height)}:${String(seed)}`),
  )
const built = await buildSnapshot({ encodeImage: stubImage })
const fetcher: Fetcher = (url) => {
  const bytes = built.files.get(url.slice("/cdn/".length))
  return Promise.resolve(
    bytes ? new Response(bytes.slice().buffer) : new Response(null, { status: 404 }),
  )
}
const client = createSnapshotClient("/cdn", { fetch: fetcher })
await client.load()
const snapshot = client.snapshot()
if (!snapshot) throw new Error("fixture did not load")
const catalog = createCatalog(snapshot)

describe("catalog", () => {
  it("flattens every card into one search entry with names in every shipped language", () => {
    expect(catalog.entries).toHaveLength(catalog.index.cards.length)
    const recruit = catalog.entries.find((entry) => entry.cardId === "c:bp01-001")
    expect(recruit).toMatchObject({
      classCode: "royal",
      setId: "set:bp01",
      defaultPrinting: { jp: "p:bp01-001", en: "p:bp01-001-en" },
    })
    expect(recruit?.names.map((name) => name.lang).sort()).toEqual(["en", "ja", "zh-Hant"])
    expect(recruit?.printings.map((printing) => printing.cardNo)).toEqual([
      "BP01-001",
      "BP01EN-001",
    ])
    expect(catalog.sets).toEqual(new Set(["bp01", "pr", "sd01"]))
    expect(catalog.classCodes).toContain("nemesis")
  })

  it("answers the three loose card numbers of the design and marks alias hits", () => {
    for (const typed of ["bp01-51", "BP01 051", "BP01-051EN"]) {
      expect(catalog.suggest(typed, "jp", 5)[0]).toMatchObject({
        cardId: "c:bp01-051",
        field: "cardNo",
      })
    }
    expect(catalog.suggest("BP01-051EN", "jp", 5)[0]?.printingId).toBe("p:bp01-051-en")
    const alias = catalog.suggest("テンプラー", "jp", 5)
    expect(alias[0]).toMatchObject({ cardId: "c:bp01-051", field: "alias" })
  })

  it("summarises a printing's front face for the list cell", () => {
    expect(catalog.summary("p:bp01-001")).toMatchObject({
      cardId: "c:bp01-001",
      cardNo: "BP01-001",
      region: "jp",
      classCode: "royal",
      cost: 1,
      attack: 1,
      defense: 1,
    })
    expect(catalog.summary("p:bp01-001")?.name.original.lang).toBe("ja")
    expect(catalog.summary("p:bp01-001-en")?.name.original.lang).toBe("en")
    expect(catalog.summary("p:nope")).toBeUndefined()
  })

  it("lists results in snapshot order and filters by class", () => {
    const all = catalog.results(DEFAULT_QUERY, "jp")
    expect(all).toHaveLength(catalog.entries.length)
    expect(all[0]).toEqual({ key: "c:bp01-001", printingId: "p:bp01-001" })
    const bishops = catalog.results({ ...DEFAULT_QUERY, classes: ["bishop"] }, "en")
    expect(bishops.every((item) => catalog.summary(item.printingId)?.classCode === "bishop")).toBe(
      true,
    )
    expect(bishops[0]?.printingId).toBe("p:bp01-050-en")
  })

  it("labels classes in the UI language and caches one catalog per snapshot", () => {
    expect(catalog.classLabel("elf", "zh-Hant")).toBe("精靈")
    expect(catalog.classLabel("elf", "ja")).toBe("エルフ")
    expect(catalog.classLabel("nope", "ja")).toBe("nope")
    expect(catalogOf(snapshot)).toBe(catalogOf(snapshot))
    expect(catalog.normalizerMatches).toBe(true)
  })
})
