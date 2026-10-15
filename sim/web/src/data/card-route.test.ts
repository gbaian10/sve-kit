// @vitest-environment node
import { describe, expect, it } from "vitest"

import { buildSnapshot } from "../../scripts/fixture/build"
import { memoryCache } from "../test-utils/cache"
import { locateCard } from "./card-route"
import { createSnapshotClient } from "./client"

const built = await buildSnapshot({ encodeImage: () => Promise.resolve(new Uint8Array([1])) })
function client(region: "jp" | "en" = "jp") {
  const requests: string[] = []
  const files = new Map(built.files)
  const client = createSnapshotClient("https://cdn.test", {
    region,
    cacheStorage: memoryCache(),
    fetch: (url) => {
      const path = url.slice("https://cdn.test/".length)
      requests.push(path)
      const bytes = files.get(path)
      return Promise.resolve(
        bytes ? new Response(bytes.slice().buffer) : new Response(null, { status: 404 }),
      )
    },
  })
  return { client, requests, files }
}

describe("catalog-based cold card routes", () => {
  it("opens a non-default lowercase printing through a permanent alias", async () => {
    const h = client()
    const result = await locateCard(h.client, { namespace: "official", key: "BP01-002A" })
    expect(result.printing.cardNo).toBe("BP01-002a")
    expect(result.printing.intId).toBeGreaterThan(0)
    expect(result.catalog.summary(result.printing.id)?.cardNo).toBe("BP01-002a")
  })
  it("opens exact EN identity without loading the other catalog", async () => {
    const h = client("en")
    const result = await locateCard(h.client, { namespace: "official", key: "BP01EN-002" })
    expect(result.printing.region).toBe("en")
    const snapshot = h.client.snapshot()
    const jpOnly = [...(snapshot?.files.values() ?? [])]
      .filter((file) => file["purpose"] === "catalog" && file["region"] === "jp")
      .map((file) => file["path"])
    expect(h.requests.some((path) => jpOnly.includes(path))).toBe(false)
  })
  it("follows the URL key when navigation state predates a renumbering", async () => {
    const h = client()
    const current = await locateCard(h.client, { namespace: "official", key: "BP01-002a" })
    const result = await locateCard(h.client, {
      namespace: "official",
      key: "BP01-002A",
      hint: current.printing,
    })
    expect(result.printing.id).toBe(current.printing.id)
  })
  it("reads printing integers of the first region only before falling back", async () => {
    const h = client()
    const { printing } = await locateCard(h.client, { namespace: "official", key: "BP01-002a" })
    const result = await locateCard(h.client, {
      namespace: "provisional",
      key: String(printing.intId),
    })
    expect(result.printing.id).toBe(printing.id)
    const en = [...(h.client.snapshot()?.files.values() ?? [])]
      .filter((file) => file["region"] === "en")
      .map((file) => file["path"])
    expect(h.requests.some((path) => en.includes(path))).toBe(false)
  })
  it("rejects an incorrect navigation owner and corrupt bytes as data errors", async () => {
    const h = client()
    const result = await locateCard(h.client, { namespace: "official", key: "BP01-002" })
    await expect(
      locateCard(h.client, {
        namespace: "official",
        key: result.printing.cardNo,
        hint: { ...result.printing, printingOwner: "set:wrong" },
      }),
    ).rejects.toMatchObject({ kind: "reference" })
    const broken = client()
    await broken.client.load()
    const snapshot = broken.client.snapshot()
    const aliases = [...(snapshot?.files.values() ?? [])].find((file) =>
      JSON.stringify(file["row_counts"]).includes("card_route_alias"),
    )
    if (!aliases || typeof aliases["path"] !== "string") throw new Error("missing routing file")
    broken.files.set(aliases["path"], new Uint8Array([0]))
    await expect(
      locateCard(broken.client, { namespace: "official", key: "BP01-002A" }),
    ).rejects.toThrow(/hash/u)
  })
})
