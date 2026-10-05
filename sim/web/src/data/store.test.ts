// @vitest-environment node
import { describe, expect, it } from "vitest"

import { buildSnapshot } from "../../scripts/fixture/build"
import { type Fetcher } from "./cdn"
import { createSnapshotClient } from "./client"
import { type JsonObject, stringValue } from "./format-v1/json"
import { loadImagePage } from "./images"
import { bucketOf, createLocator, GLOBAL_OWNER, homeSetOwner } from "./locator"
import { createCardIndex } from "./store"
import { createTextResolver } from "./text"

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
const snapshot = client.snapshot()
if (!snapshot) throw new Error("fixture did not load")
const index = createCardIndex(snapshot)

describe("locator", () => {
  it("maps every fragment identity to exactly one file", () => {
    const locate = createLocator(snapshot.files)
    for (const [key, file] of snapshot.files) {
      for (const row of file["row_counts"] as JsonObject[]) {
        expect(
          locate({
            table: stringValue(row["table"]),
            owner: row["owner"] as JsonObject,
            bucket: Number(row["bucket"]),
            partition: row["partition"] as "bootstrap" | "detail" | "history",
          }),
        ).toBe(key)
      }
    }
    expect(
      locate({
        table: "printing",
        owner: homeSetOwner("set:nope"),
        bucket: 0,
        partition: "detail",
      }),
    ).toBeUndefined()
    expect(bucketOf(["c:example"], 4)).toBe(3)
    expect(bucketOf(["c:example"], 1)).toBe(0)
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
    expect(index.support("c:bp01-020")?.["shared"]).toMatchObject({ status: "load_rejected" })
    expect(index.family("set:bp01")?.["public_code"]).toBe("BP01")
    expect(index.product("prod:bp01-jp")?.["region"]).toBe("jp")
    expect(index.currentRevision("f:bp01en-090", "jp")).toBeUndefined()
  })
})

describe("text resolver", () => {
  it("answers bootstrap names at once and effects after loading their detail file", async () => {
    const text = createTextResolver(client, index)
    const revision = index.currentRevision("f:bp01-001", "jp")
    const nameUnit = stringValue(revision?.["name_unit_id"])
    expect(text.textOf(nameUnit)).toBe("試作の見習い兵")
    const detail = await client.fragments(
      createLocator(snapshot.files)({
        table: "face_revision",
        owner: homeSetOwner("set:bp01"),
        bucket: bucketOf(["c:bp01-001"], 64),
        partition: "detail",
      }) ?? "",
    )
    const full = detail
      .filter((fragment) => fragment.table === "face_revision")
      .flatMap((fragment) => fragment.rows)
      .find((row) => row["id"] === revision?.["id"])
    const effectUnit = stringValue(full?.["effect_unit_id"])
    expect(text.textOf(effectUnit)).toBeUndefined()
    await text.ensure([effectUnit, nameUnit])
    expect(text.textOf(effectUnit)).toBe("【ファンファーレ】自分のリーダーを1回復する。")
    expect(
      requests.filter(
        (path) =>
          path ===
          snapshot.files.get(
            createLocator(snapshot.files)({
              table: "text_unit",
              owner: GLOBAL_OWNER,
              bucket: bucketOf([effectUnit], 64),
              partition: "detail",
            }) ?? "",
          )?.["path"],
      ),
    ).toHaveLength(1)
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
