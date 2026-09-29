// @vitest-environment node
import { describe, expect, it } from "vitest"

import { buildSnapshot } from "../../scripts/fixture/build"
import { parseCardText } from "../domain/cardText"
import { resolveCardRoute } from "../domain/route"
import { createRouteLookups, loadCardView } from "./cardView"
import { createCatalog } from "./catalog"
import type { Fetcher } from "./cdn"
import { createSnapshotClient } from "./client"
import { globalDetailOf } from "./detail"
import { integerValue, stringValue } from "./format-v1/json"

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

describe("loadCardView", () => {
  it("resolves the effect with icons and keywords and the Traditional Chinese row", async () => {
    const view = await loadCardView(client, catalog, "p:bp01-051", "zh-TW")
    if (!view) throw new Error("no view")
    expect(view).toMatchObject({
      cardNo: "BP01-051",
      region: "jp",
      setCode: "BP01",
      editions: { jp: "p:bp01-051", en: "p:bp01-051-en" },
    })
    const face = view.faces[0]
    expect(face?.effect?.original.lang).toBe("ja")
    expect(face?.effect?.translation).toMatchObject({ lang: "zh-Hant", label: "project" })
    const segments = parseCardText(face?.effect?.original.text ?? "", "ja", view.vocabulary)
    expect(segments.map((segment) => segment.kind)).toEqual([
      "keyword",
      "break",
      "symbol",
      "symbol",
      "text",
    ])
    expect(segments[2]).toMatchObject({ code: "fanfare" })
    expect(segments[3]).toMatchObject({ code: "cost", parameter: "2" })
    expect(view.symbolLocalization("sym:cost", "zh-Hant")?.["name"]).toBe("費用")
    // A pure-variable symbol (uint: null) parses its variable and declares no bounds.
    const xvar = view.vocabulary.spellings.find(
      (item) => item.code === "xvar" && item.lang === "ja",
    )
    expect(xvar).toMatchObject({ parse: "variable", variables: ["X"] })
    expect(xvar?.minimum).toBeUndefined()
    expect(
      parseCardText("{変数X}{変数Y}", "ja", view.vocabulary).map((segment) => segment.kind),
    ).toEqual(["symbol", "unknown"])
    const ward = view.keyword("kw:ward")
    expect(ward?.name("zh-Hant")).toBe("守護")
    expect(await ward?.definition()).toBeTruthy()
  })

  it("uses the printing's region for the original and the English counterpart for the en UI", async () => {
    const jp = await loadCardView(client, catalog, "p:bp01-051", "en")
    expect(jp?.faces[0]?.effect?.translation).toMatchObject({ lang: "en", label: "official" })
    const en = await loadCardView(client, catalog, "p:bp01-051-en", "en")
    expect(en?.region).toBe("en")
    expect(en?.faces[0]?.effect?.original.lang).toBe("en")
    expect(en?.faces[0]?.effect?.translation).toBeNull()
    expect(en?.faces[0]?.effect?.notices).toEqual([])
  })

  it("reports a missing translation and handles back faces and unknown printings", async () => {
    // BP01-011 has no English name or effect at all.
    const jpOnly = await loadCardView(client, catalog, "p:bp01-011", "en")
    expect(jpOnly?.editions).toEqual({ jp: "p:bp01-011" })
    expect(jpOnly?.faces[0]?.effect?.translation).toBeNull()
    expect(jpOnly?.faces[0]?.effect?.notices).toContain("missing")
    expect(await loadCardView(client, catalog, "p:nope", "zh-TW")).toBeUndefined()
    const double = catalog.index.cards.find((card) => card["layout"] === "double_faced")
    if (!double) throw new Error("fixture has no double-faced card")
    const printing = catalog.index.printingsOf(stringValue(double["id"]))[0]
    const view = await loadCardView(client, catalog, stringValue(printing?.["id"]), "zh-TW")
    expect(view?.faces.map((face) => face.side)).toEqual(["front", "back"])
  })

  it("feeds the route resolver with aliases, overrides and provisional ids", async () => {
    const global = globalDetailOf(client, catalog.index)
    const lookupsFor = (value: string, namespace: "official" | "provisional" = "official") =>
      createRouteLookups(catalog, global, { namespace, value })
    expect(resolveCardRoute("BP01-002A", undefined, await lookupsFor("BP01-002A"))).toEqual({
      kind: "redirect",
      to: "/cards/BP01-002a",
    })
    expect(resolveCardRoute("BP01-002", undefined, await lookupsFor("BP01-002"))).toMatchObject({
      kind: "redirect",
    })
    expect(resolveCardRoute("BP01-002", "試作の隊長", await lookupsFor("BP01-002"))).toMatchObject({
      kind: "found",
      printingId: "p:bp01-002",
    })
    const intId = integerValue(catalog.index.printing("p:bp01-001")?.["int_id"])
    expect((await lookupsFor(String(intId), "provisional")).printingByIntId(intId)).toBe(
      "p:bp01-001",
    )
  })

  it("fetches only the buckets a card needs", async () => {
    const requested: string[] = []
    const recording = createSnapshotClient("/cdn", {
      fetch: (url) => {
        requested.push(url.slice("/cdn/".length))
        return fetcher(url)
      },
    })
    await recording.load()
    const snapshot2 = recording.snapshot()
    if (!snapshot2) throw new Error("no snapshot")
    const catalog2 = createCatalog(snapshot2)
    requested.length = 0
    await loadCardView(recording, catalog2, "p:bp01-051", "zh-TW")
    const files = [...snapshot2.files.entries()]
      .filter(([, file]) => file["role"] === "text")
      .map(([key, file]) => [key, stringValue(file["path"])] as const)
    const fetched = files.filter(([, path]) => requested.includes(path)).map(([key]) => key)
    // The home set's file and the global file; other sets' text files are never touched.
    expect(fetched.sort()).toEqual(["text/bp01", "text/global"])
  })
})
