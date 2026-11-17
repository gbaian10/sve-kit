// @vitest-environment node
import { describe, expect, it } from "vitest"

import { buildSnapshot } from "../../scripts/fixture/build"
import { DEFAULT_QUERY } from "../domain/query/model"
import { catalogOf, prepareCatalog } from "./catalog"
import type { Fetcher } from "./cdn"
import { createSnapshotClient } from "./client"
import type { Row } from "./format-v3/decode"
import { stringValue } from "./format-v3/json"
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
const catalog = catalogOf(snapshot)

describe("catalog", () => {
  it("keeps prepared summaries and search results equal regardless of which is read first", async () => {
    const candidate = { ...snapshot }
    await prepareCatalog(candidate, () => undefined)
    const prepared = catalogOf(candidate)
    const eager = catalogOf(candidate)
    const summaries = new Map(
      eager.index.cards.flatMap((card) =>
        eager.index.printingsOf(stringValue(card["id"])).map((printing) => {
          const id = stringValue(printing["id"])
          const summary = prepared.summary(id)
          expect(summary).toEqual(eager.summary(id))
          return [id, summary] as const
        }),
      ),
    )
    expect(prepared.summary("p:missing")).toBeUndefined()
    expect(prepared.entries).toEqual(eager.entries)
    for (const region of ["jp", "en"] as const) {
      expect(prepared.results(DEFAULT_QUERY, region)).toEqual(eager.results(DEFAULT_QUERY, region))
      expect(prepared.suggest("BP01", region, 20)).toEqual(eager.suggest("BP01", region, 20))
      expect(prepared.suggest("テンプラー", region, 5)).toEqual(
        eager.suggest("テンプラー", region, 5),
      )
    }
    for (const [id, summary] of summaries) expect(prepared.summary(id)).toBe(summary)
    expect(catalogOf(candidate)).toBe(prepared)
    expect(prepared.entries).toBe(prepared.entries)

    const searchFirst = { ...snapshot }
    await prepareCatalog(searchFirst, () => undefined)
    const searched = catalogOf(searchFirst)
    expect(searched.entries).toEqual(eager.entries)
    for (const [id, summary] of summaries) expect(searched.summary(id)).toEqual(summary)
    expect(searched.summary("p:missing")).toBeUndefined()
  })

  it("labels classes in the UI language and caches one catalog per snapshot", () => {
    expect(catalog.classLabel("elf", "zh-Hant")).toBe("精靈")
    expect(catalog.classLabel("elf", "ja")).toBe("エルフ")
    expect(catalog.classLabel("nope", "ja")).toBe("nope")
    expect(catalogOf(snapshot)).toBe(catalogOf(snapshot))
    expect(catalog.normalizerMatches).toBe(true)
  })
})

describe("printed names of merged printings", () => {
  it("lists a printing by its own printed name and keeps the card's name as treated-as", () => {
    const donor = catalog.index.currentRevision("f:bp01-051", "jp")
    const own = donor?.["name_unit_id"] ?? null
    const translations = ((donor?.["translations"] ?? []) as Row[]).filter(
      (item) => item["field"] === "name",
    )
    const revised = {
      ...snapshot,
      bootstrap: snapshot.bootstrap.map((fragment) => ({
        ...fragment,
        rows: fragment.rows.map((row) =>
          row["id"] === "p:bp01-001" && fragment.table === "printing"
            ? {
                ...row,
                faces: (row["faces"] as Row[]).map((face) => ({
                  ...face,
                  name_unit_id: own,
                  name_translations: translations,
                })),
              }
            : row,
        ),
      })),
    }
    const summary = catalogOf(revised).summary("p:bp01-001")
    expect(summary?.name.original.text).toBe(catalog.summary("p:bp01-051")?.name.original.text)
    expect(summary?.name.translations["zh-Hant"]).toBe(
      catalog.summary("p:bp01-051")?.name.translations["zh-Hant"],
    )
    expect(summary?.name.translations["zh-Hant"]).toBeTruthy()
    expect(summary?.treatedAs?.original.text).toBe("試作の見習い兵")
    expect(catalogOf(revised).summary("p:bp01-001-en")?.treatedAs).toBeUndefined()
  })
})

describe("manual names spelled like the card's name", () => {
  it("shows the card's name and translation instead of an untranslated generic name", () => {
    // A manual "EP" on the EP card is no name of its own; a proper name would stay as printed.
    const own = catalog.index.currentRevision("f:bp01-001", "jp")?.["name_unit_id"] ?? null
    const revised = {
      ...snapshot,
      bootstrap: snapshot.bootstrap.map((fragment) => ({
        ...fragment,
        rows: fragment.rows.map((row) =>
          row["id"] === "p:bp01-001" && fragment.table === "printing"
            ? { ...row, manual_name_unit_id: own }
            : row,
        ),
      })),
    }
    const summary = catalogOf(revised).summary("p:bp01-001")
    expect(summary?.name.original.text).toBe("試作の見習い兵")
    expect(summary?.name.translations["zh-Hant"]).toBe("試作見習兵")
    expect(summary?.treatedAs).toBeUndefined()
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
    const pending = catalogOf(revised)
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
    expect(catalogOf(revised).summary("p:bp01-001-en")?.name.original).toEqual({
      lang: "en",
      text: "BP01EN-001",
    })
  })
})
