// @vitest-environment node
import { describe, expect, it } from "vitest"

import { buildSnapshot } from "../../scripts/fixture/build"
import { DEFAULT_QUERY } from "../domain/query/model"
import { catalogOf, createCatalog } from "./catalog"
import type { Fetcher } from "./cdn"
import { createSnapshotClient } from "./client"
import { faceBasics } from "./face-basics"
import type { Row } from "./format-v3/decode"
import { BootstrapColumns } from "./search/bootstrap"
import { SearchIndex } from "./search/index"

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
await client.prefetchPrintings?.()
await client.selectRegion?.("en")
await client.prefetchPrintings?.()
await client.selectRegion?.("jp")
await client.prefetchPrintings?.()
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
    expect(recruit?.printings.map((printing) => printing.cardNo).sort()).toEqual([
      "BP01-001",
      "BP01EN-001",
    ])
    expect(catalog.sets).toEqual(new Set(["bp01", "pr", "sd01"]))
    expect([...catalog.classCodes].sort()).toEqual([
      "bishop",
      "dragon",
      "elf",
      "nightmare",
      "royal",
      "witch",
    ])
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

describe("pending display in the list", () => {
  it.each([true, false])("keeps a pending card visible with display=%s", (available) => {
    const revised = {
      ...snapshot,
      bootstrap: snapshot.bootstrap.map((fragment) => ({
        ...fragment,
        rows: fragment.rows.map((row) =>
          row["id"] === "f:bp01-001" && fragment.value["region"] === "jp"
            ? {
                ...row,
                current: (row["current"] as { region: string }[]).filter(
                  (entry) => entry.region !== "jp",
                ),
                wording: [
                  {
                    region: "jp",
                    state: "pending",
                    display: {
                      revision_id: available
                        ? (catalog.index.currentRevision("f:bp01-001", "jp")?.["id"] ?? null)
                        : null,
                      basis: available ? "latest_known_release" : "candidates",
                    },
                    common_name: available
                      ? null
                      : {
                          name_unit_id:
                            catalog.index.currentRevision("f:bp01-001", "jp")?.["name_unit_id"] ??
                            "",
                          translations:
                            catalog.index.currentRevision("f:bp01-001", "jp")?.["translations"] ??
                            [],
                        },
                    candidates: [],
                    undated_printing_ids: ["p:bp01-001"],
                  },
                ],
              }
            : row,
        ),
      })),
    }
    const pending = createCatalog(revised)
    expect(pending.index.currentRevision("f:bp01-001", "jp")).toBeUndefined()
    expect(pending.index.wording("f:bp01-001", "jp")?.["undated_printing_ids"]).toEqual([
      "p:bp01-001",
    ])
    const summary = pending.summary("p:bp01-001")
    expect(summary).toBeDefined()
    expect(summary?.name.original.text).toBe("試作の見習い兵")
    expect(summary?.cost).toBe(available ? 1 : null)
    expect(summary?.name.translations["zh-Hant"]).toBe("試作見習兵")
    expect(pending.suggest("試作の見習い兵", "jp", 5)[0]?.cardId).toBe("c:bp01-001")
    const columns = new BootstrapColumns()
    columns.ingest(revised.bootstrap)
    columns.seal()
    const worker = new SearchIndex(columns, "jp")
    expect(worker.suggest("試作見習兵").items[0]?.summary.name).toEqual(summary?.name)
    expect(worker.recent(["p:bp01-001"])[0]?.summary?.name).toEqual(summary?.name)
    expect(summary?.wordingPending).toBe(true)
    expect(summary?.name.original.lang).toBe("ja")
    expect(pending.index.wording("f:bp01-001", "en")).toBeUndefined()
    expect(pending.summary("p:bp01-001-en")?.wordingPending).toBe(false)
    expect(
      pending.results(DEFAULT_QUERY, "jp").some((item) => item.printingId === "p:bp01-001"),
    ).toBe(true)
  })
  it("uses the printing observation's name and stats without an adopted or displayed revision", () => {
    const original = catalog.index.currentRevision("f:bp01-001", "jp")
    const revised = {
      ...snapshot,
      bootstrap: snapshot.bootstrap.map((fragment) => ({
        ...fragment,
        rows: fragment.rows.map((row) =>
          row["id"] === "f:bp01-001" && fragment.value["region"] === "jp"
            ? {
                ...row,
                current: (row["current"] as { region: string }[]).filter(
                  (entry) => entry.region !== "jp",
                ),
                wording: [
                  {
                    region: "jp",
                    display: { revision_id: null, basis: "candidates" },
                    candidates: [
                      { printing_id: "p:bp01-001", revision_id: original?.["id"] ?? null },
                    ],
                    undated_printing_ids: [],
                  },
                ],
              }
            : row,
        ),
      })),
    }
    const own = createCatalog(revised).summary("p:bp01-001")
    expect(own?.name.original.text).toBe("試作の見習い兵")
    expect(own?.cost).toBe(1)
  })
  it("prefers current over a pending display", () => {
    const revision = catalog.index.currentRevision("f:bp01-001", "jp")
    expect(catalog.index.displayRevision("f:bp01-001", "jp")).toEqual(revision)
    expect(catalog.index.displayRevision("f:nope", "jp")).toBeUndefined()
  })
  it("keeps an English card-number fallback tagged as English", () => {
    const revised = {
      ...snapshot,
      bootstrap: snapshot.bootstrap.map((fragment) => ({
        ...fragment,
        rows: fragment.rows.map((row) =>
          row["id"] === "f:bp01-001" && fragment.value["region"] === "en"
            ? {
                ...row,
                current: (row["current"] as { region: string }[]).filter(
                  (entry) => entry.region !== "en",
                ),
                wording: [{ region: "en", display: { revision_id: null, basis: "candidates" } }],
              }
            : row,
        ),
      })),
    }
    expect(createCatalog(revised).summary("p:bp01-001-en")?.name.original).toEqual({
      lang: "en",
      text: "BP01EN-001",
    })
  })
})

describe("current name translations", () => {
  it("retains a low-confidence selected name in display and search, independent of Japanese source basis", () => {
    const revised = {
      ...snapshot,
      bootstrap: snapshot.bootstrap.map((fragment) => ({
        ...fragment,
        rows: fragment.rows.map((row) => {
          if (fragment.table === "translation")
            return { ...row, origin: "machine", low_confidence: true }
          if (fragment.table === "face_revision")
            return {
              ...row,
              translations: (row["translations"] as { basis: string }[]).map((value) => ({
                ...value,
                basis: "jp_source",
              })),
            }
          return row
        }),
      })),
    }
    const current = createCatalog(revised)
    const name = current.summary("p:bp01-001")?.name
    expect(name?.translations["zh-Hant"]).toBe(
      catalog.summary("p:bp01-001")?.name.translations["zh-Hant"],
    )
    expect(name?.translationQuality?.["zh-Hant"]).toEqual({
      lowConfidence: true,
      jpSource: true,
    })
    expect(current.entries.find((entry) => entry.cardId === "c:bp01-001")?.names).toEqual(
      catalog.entries.find((entry) => entry.cardId === "c:bp01-001")?.names,
    )
  })
})

describe("pending card basic fields", () => {
  const original = catalog.index.currentRevision("f:bp01-001", "jp")
  if (!original) throw new Error("missing synthetic revision")
  const candidates = (rows: Row[], refs: Row[], selected?: string) => {
    return createCatalog({
      ...snapshot,
      bootstrap: snapshot.bootstrap.map((fragment) => ({
        ...fragment,
        rows: fragment.rows.flatMap((row) => {
          if (row["id"] === original["id"]) return rows
          if (row["id"] !== "f:bp01-001" || fragment.value["region"] !== "jp") return [row]
          return [
            {
              ...row,
              current: selected
                ? [{ region: "jp", revision_id: selected, basis: "latest_known_release" }]
                : [],
              wording: [
                {
                  region: "jp",
                  display: {
                    revision_id: selected ?? null,
                    basis: selected ? "current" : "candidates",
                  },
                  common_name: null,
                  candidates: refs,
                },
              ],
            },
          ]
        }),
      })),
    }).index
  }
  const one: Row = {
    ...original,
    id: "r:one",
    cost: 3,
    attack: 4,
    defense: 5,
    traits: ["synthetic"],
  }
  const two: Row = { ...one, id: "r:two" }
  it("shows agreed fields while keeping wording pending", () => {
    const index = candidates(
      [one, two],
      [
        { printing_id: "p:a", revision_id: "r:one" },
        { printing_id: "p:b", revision_id: "r:two" },
      ],
    )
    expect(faceBasics(index, "f:bp01-001", "jp", "p:b").values).toMatchObject({
      class_code: "royal",
      type_code: "follower",
      traits: ["synthetic"],
      cost: 3,
      attack: 4,
      defense: 5,
    })
    expect(index.currentRevision("f:bp01-001", "jp")).toBeUndefined()
    expect(index.displayRevision("f:bp01-001", "jp")).toBeUndefined()
  })
  it("locates the exact printing when candidate stats differ", () => {
    const index = candidates(
      [one, { ...two, cost: 8 }],
      [
        { printing_id: "p:a", revision_id: "r:one" },
        { printing_id: "p:b", revision_id: "r:two" },
      ],
    )
    expect(faceBasics(index, "f:bp01-001", "jp", "p:b").values["cost"]).toBe(8)
  })
  it("uses this printing's stats when a different shared revision is current", () => {
    const index = candidates(
      [one, { ...two, cost: 8 }],
      [
        { printing_id: "p:a", revision_id: "r:one" },
        { printing_id: "p:b", revision_id: "r:two" },
      ],
      "r:one",
    )
    expect(index.currentRevision("f:bp01-001", "jp")?.["cost"]).toBe(3)
    const basics = faceBasics(index, "f:bp01-001", "jp", "p:b")
    expect(basics.values["cost"]).toBe(8)
    expect(basics.revision?.["id"]).toBe("r:two")
  })
  it("keeps conflicting observations of one printing unresolved", () => {
    const index = candidates(
      [one, { ...two, cost: 8 }],
      [
        { printing_id: "p:b", revision_id: "r:one" },
        { printing_id: "p:b", revision_id: "r:two" },
      ],
    )
    const basics = faceBasics(index, "f:bp01-001", "jp", "p:b")
    expect(basics.values["cost"]).toBeUndefined()
    expect(basics.values["attack"]).toBe(4)
    expect(basics.revision).toBeUndefined()
  })
  it("does not infer agreement from partially loaded candidates", () => {
    const index = candidates(
      [one],
      [
        { printing_id: "p:a", revision_id: "r:one" },
        { printing_id: "p:b", revision_id: "r:unloaded" },
      ],
    )
    expect(faceBasics(index, "f:bp01-001", "jp", "p:b").values).toEqual({})
  })
})
