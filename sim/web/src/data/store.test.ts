// @vitest-environment node
import { describe, expect, it } from "vitest"

import { buildSnapshot } from "../../scripts/fixture/build"
import { createAnnotatedTextResolver } from "./annotated-text"
import { type Fetcher } from "./cdn"
import { createSnapshotClient } from "./client"
import { type JsonObject, type JsonValue, stringValue } from "./format-v3/json"
import { loadImagePage } from "./images"
import { createLocator, GLOBAL_OWNER, homeSetOwner } from "./locator"
import { createCardIndex } from "./store"

const stubImage = ({ width, height, seed }: { width: number; height: number; seed: number }) =>
  Promise.resolve(
    new TextEncoder().encode(`stub-webp:${String(width)}x${String(height)}:${String(seed)}`),
  )
const built = await buildSnapshot({ encodeImage: stubImage })
const requests: string[] = []
const fetcher: Fetcher = (url) => {
  const path = url.slice("/cdn/".length)
  requests.push(path)
  const bytes = built.files.get(path)
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
const index = createCardIndex(snapshot)

describe("locator", () => {
  it("uses sixteen bands only for global detail with null region, including inferred scope", () => {
    const files = new Map<string, JsonObject>()
    for (const [purpose, region, table, bands] of [
      ["detail", null, "qa_version", 16],
      ["detail", "jp", "qa", 8],
      ["detail", "en", "qa", 8],
      ["engine", null, "voice", 8],
    ] as const)
      for (let band = 0; band < bands; band += 1) {
        const key = `${purpose}/${region ?? "shared"}/shared/${String(band)}`
        const segment = `shared:band-${String(band)}`
        files.set(key, {
          key,
          purpose,
          region,
          segment,
          row_counts: [[table, "global", null, "detail", segment, 1]],
        })
      }
    const locate = createLocator(files)
    // SHA-256(canonical(["qa_v"])) mod 64 = 33; ["qa:c"] gives 58.
    for (const [id, shared, regional] of [
      ["qa_v", 8, 4],
      ["qa:c", 14, 7],
    ] as const) {
      const ref = { owner: GLOBAL_OWNER, entityKey: [id], partition: "detail" as const }
      expect(locate({ ...ref, table: "qa_version" })).toBe(`detail/shared/shared/${String(shared)}`)
      expect(locate({ ...ref, table: "qa_version", region: "jp" })).toBe(
        `detail/shared/shared/${String(shared)}`,
      )
      for (const region of ["jp", "en"])
        expect(locate({ ...ref, table: "qa", region })).toBe(
          `detail/${region}/shared/${String(regional)}`,
        )
      expect(locate({ ...ref, table: "voice" })).toBe(`engine/shared/shared/${String(regional)}`)
    }
  })

  it("maps every fragment identity to exactly one file", () => {
    const locate = createLocator(snapshot.files)
    for (const [key, file] of snapshot.files) {
      for (const row of file["row_counts"] as JsonValue[][]) {
        expect(
          locate({
            table: stringValue(row[0]),
            owner: { kind: row[1] ?? null, id: row[2] ?? null },
            purpose: stringValue(file["purpose"]),
            region: file["region"] as string | null,
            segment: stringValue(row[4]),
            entityKey: [],
            partition: row[3] as "bootstrap" | "detail" | "history",
          }),
        ).toBe(key)
      }
    }
    expect(
      locate({
        table: "printing",
        owner: homeSetOwner("set:nope"),
        entityKey: [],
        partition: "detail",
      }),
    ).toBeUndefined()
  })

  it("finds a visible face in its catalog beside the same family's region-state", () => {
    const file = (key: string, purpose: string, segment: string): [string, JsonObject] => [
      key,
      {
        key,
        purpose,
        region: "en",
        segment,
        row_counts: [["face", "home_set", "BP02", "bootstrap", segment, 1]],
      },
    ]
    const locate = createLocator(
      new Map([
        file("catalog/en/family-BP02/0", "catalog", "family-BP02"),
        file("region-state/en/family-BP02/0", "region-state", "family-BP02:band-0"),
      ]),
    )
    expect(
      locate({
        table: "face",
        owner: homeSetOwner("BP02"),
        entityKey: ["c:bp02-001"],
        region: "en",
        partition: "bootstrap",
      }),
    ).toBe("catalog/en/family-BP02/0")
  })
})

describe("card index", () => {
  it("answers the bootstrap lookups the pages need", () => {
    expect(index.cards.length).toBe(built.counts["card"])
    const card = index.card("c:bp01-030")
    expect(card?.["layout"]).toBe("double_faced")
    expect(index.facesOf("c:bp01-030").map((face) => face["side"])).toEqual(["front", "back"])
    expect(index.printingsOf("c:bp01-002")).toHaveLength(3)
    expect(index.printingByCardNo("jp", "BP01-002a")?.["variant_key"]).toBe("alt")
    const revision = index.currentRevision("f:bp01-001", "jp")
    expect(revision?.["type_code"]).toBe("follower")
    expect(revision?.["effect_unit_id"]).toBeUndefined()
    expect(index.textUnit(stringValue(revision?.["name_unit_id"]))?.["text"]).toBe("試作の見習い兵")
    expect(index.vocabulary("class", "royal")?.["active"]).toBe(true)
    expect(index.keyword("kw:fanfare")?.["code"]).toBe("fanfare")
    expect(index.support("c:bp01-020")).toBeUndefined()
    expect(index.family("set:bp01")?.["public_code"]).toBe("BP01")
    expect(index.product("prod:bp01-jp")?.["region"]).toBe("jp")
    expect(index.currentRevision("f:bp01en-090", "jp")).toBeUndefined()
  })
})

describe("file-local text", () => {
  it("resolves an effect through its exact owner without a global dictionary lookup", async () => {
    const revision = index.currentRevision("f:bp01-001", "jp")
    const result = await createAnnotatedTextResolver(client).resolve(
      {
        owner: { kind: "face_revision", id: revision?.["id"] ?? null },
        field: "effect",
        ordinal: null,
      },
      "ja",
    )
    expect(result?.original.unit["text"]).toBe("【ファンファーレ】自分のリーダーを1回復する。")
  })
})

describe("images", () => {
  it("indexes printing faces to versioned integer-ID variants", async () => {
    const images = await loadImagePage(client, [
      { printingId: "p:bp01-001", faceId: "f:bp01-001" },
      { printingId: "p:bp01-040", faceId: "f:bp01-040" },
    ])
    const source = images.cardImage("p:bp01-001", "f:bp01-001")
    expect(source?.width).toBe(459)
    expect(source?.src.startsWith("/cdn/images/card_l/")).toBe(true)
    expect(source?.srcSet.split(", ")).toHaveLength(3)
    expect(source?.srcSet).toContain(" 128w")
    expect(images.cardImage("p:bp01-040", "f:bp01-040")).toBeUndefined()
    expect(images.asset("p:bp01-040", "f:bp01-040")?.["publication_state"]).toBe("pending")
  })
})
