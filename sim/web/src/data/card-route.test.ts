// @vitest-environment node
import { describe, expect, it } from "vitest"

import { buildSnapshot } from "../../scripts/fixture/build"
import { memoryCache } from "../test-utils/cache"
import { locateCard } from "./card-route"
import { catalogOf, createCatalog } from "./catalog"
import { createSnapshotClient } from "./client"
import { arrayValue, canonical, objectValue, parseJson, stringValue } from "./format-v3/json"
import { columns, rowType } from "./format-v3/schema"
import { digest } from "./format-v3/sha256"

const built = await buildSnapshot({ encodeImage: () => Promise.resolve(new Uint8Array([1])) })
function client(region: "jp" | "en" = "jp", entry: "index" | "preview" = "index") {
  const requests: string[] = []
  const files = new Map(built.files)
  const client = createSnapshotClient("https://cdn.test", {
    region,
    entry,
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

function routingClient(table: "card_route_alias" | "route_override", row: readonly string[]) {
  const h = client("jp", "preview")
  const manifest = structuredClone(built.manifest)
  const files = arrayValue(manifest["files"]).map((file) => objectValue(file))
  const file = files.find((file) => JSON.stringify(file["row_counts"]).includes(table))
  if (!file) throw new Error("missing routing file")
  const path = stringValue(file["path"])
  const raw = h.files.get(path)
  if (!raw) throw new Error("missing routing bytes")
  const container = objectValue(parseJson(raw))
  const fragment = arrayValue(arrayValue(objectValue(container["tables"])[table])[0])
  fragment[4] = [[...row]]
  const bytes = canonical(container)
  file["bytes"] = bytes.length
  file["sha256"] = digest(bytes)
  h.files.set(path, bytes)
  const manifestBytes = canonical(manifest)
  h.files.set(built.manifestPath, manifestBytes)
  h.files.set(
    "snapshots/preview/current.json",
    canonical({
      manifest_path: built.manifestPath,
      manifest_sha256: digest(manifestBytes),
    }),
  )
  return { ...h, manifestFiles: files }
}

function expectCatalogsOnce(h: ReturnType<typeof routingClient>) {
  for (const region of ["jp", "en"]) {
    const paths = h.manifestFiles
      .filter((file) => file["purpose"] === "catalog" && file["region"] === region)
      .map((file) => stringValue(file["path"]))
    expect(paths.length).toBeGreaterThan(0)
    expect(h.requests.filter((path) => paths.includes(path)).sort()).toEqual(paths.sort())
  }
}

function provisionalClient() {
  const h = client("jp", "preview")
  const manifest = structuredClone(built.manifest)
  const files = arrayValue(manifest["files"]).map((file) => objectValue(file))
  const changed = new Map<string, string>()
  for (const file of files) {
    if (file["purpose"] !== "print") continue
    const path = stringValue(file["path"])
    const bytes = h.files.get(path)
    if (!bytes) throw new Error("missing printing fixture")
    const payload = objectValue(parseJson(bytes))
    const fields = columns(rowType("printing", "bootstrap"))
    let edited = false
    for (const raw of arrayValue(objectValue(payload["tables"])["printing"] ?? []))
      for (const row of arrayValue(arrayValue(raw)[4]).map((row) => arrayValue(row))) {
        if (row[fields.indexOf("id")] !== "p:bp01-002-alt") continue
        row[fields.indexOf("card_no_state")] = "provisional"
        edited = true
      }
    if (!edited) continue
    const next = canonical(payload)
    file["bytes"] = next.length
    file["sha256"] = digest(next)
    changed.set(stringValue(file["key"]), stringValue(file["sha256"]))
    h.files.set(path, next)
  }
  if (changed.size === 0) throw new Error("missing alternate printing")
  for (const file of files)
    for (const field of ["dependencies", "source_refs"])
      for (const raw of arrayValue(file[field])) {
        const ref = objectValue(raw)
        const hash = changed.get(stringValue(ref["key"]))
        if (hash) ref["sha256"] = hash
      }
  const bytes = canonical(manifest)
  h.files.set(built.manifestPath, bytes)
  h.files.set(
    "snapshots/preview/current.json",
    canonical({ manifest_path: built.manifestPath, manifest_sha256: digest(bytes) }),
  )
  return h
}

describe("catalog-based cold card routes", () => {
  it("opens an EN alias target from a JP client", async () => {
    const h = routingClient("card_route_alias", [
      "official",
      "BP01EN-002A",
      "official",
      "BP01EN-002",
      "renumbered",
    ])
    const result = await locateCard(h.client, { namespace: "official", key: "BP01EN-002A" })
    expect(result.printing).toMatchObject({
      id: "p:bp01-002-en",
      region: "en",
      cardNo: "BP01EN-002",
    })
    expectCatalogsOnce(h)
  })

  it("opens an EN override target from a JP client", async () => {
    const h = routingClient("route_override", ["BP01EN-002", "p:bp01-002-en"])
    const result = await locateCard(h.client, { namespace: "official", key: "BP01EN-002" })
    expect(result.printing).toMatchObject({
      id: "p:bp01-002-en",
      region: "en",
      cardNo: "BP01EN-002",
    })
    expectCatalogsOnce(h)
  })

  it.each([
    {
      table: "card_route_alias" as const,
      row: ["official", "BP01-002A", "official", "MISSING-001", "renumbered"],
      key: "BP01-002A",
      message: "card route alias target is missing",
    },
    {
      table: "route_override" as const,
      row: ["BP01-002", "p:missing"],
      key: "BP01-002",
      message: "route override has no matching printing",
    },
  ])(
    "reports a missing $table target after checking each region once",
    async ({ table, row, key, message }) => {
      const h = routingClient(table, row)
      await expect(locateCard(h.client, { namespace: "official", key })).rejects.toMatchObject({
        kind: "reference",
        message,
      })
      expectCatalogsOnce(h)
    },
  )

  it("rejects a cyclic alias without downloading the other region", async () => {
    const h = routingClient("card_route_alias", [
      "official",
      "OLD-001",
      "official",
      "OLD-001",
      "renumbered",
    ])
    await expect(
      locateCard(h.client, { namespace: "official", key: "OLD-001" }),
    ).rejects.toMatchObject({ kind: "reference", message: "cyclic card route alias" })
    const en = h.manifestFiles.filter((file) => file["region"] === "en").map((file) => file["path"])
    expect(h.requests.filter((path) => en.includes(path))).toEqual([])
  })

  it("opens a non-default lowercase printing through a permanent alias", async () => {
    const h = client()
    const result = await locateCard(h.client, { namespace: "official", key: "BP01-002A" })
    expect(result.printing.cardNo).toBe("BP01-002a")
    expect(result.printing.intId).toBeGreaterThan(0)
    expect(result.catalog.summary(result.printing.id)?.cardNo).toBe("BP01-002a")
  })
  it("completes summaries without changing the base snapshot or another located card", async () => {
    const h = client()
    await h.client.load()
    const initial = h.client.snapshot()
    if (!initial) throw new Error("missing snapshot")
    const before = JSON.stringify(initial.bootstrap)
    const base = catalogOf(initial)
    expect(base.summary("p:bp01-002-alt")?.intId).toBeUndefined()
    const first = await locateCard(h.client, { namespace: "official", key: "BP01-002a" })
    const second = await locateCard(h.client, { namespace: "official", key: "BP01-001" })
    expect(JSON.stringify(initial.bootstrap)).toBe(before)
    expect(h.client.snapshot()).toBe(initial)
    expect(base.summary("p:bp01-002-alt")?.intId).toBeUndefined()
    expect(first.snapshot).not.toBe(initial)
    expect(first.catalog).not.toBe(base)
    expect(catalogOf(first.snapshot)).toBe(first.catalog)
    expect(second.catalog.summary("p:bp01-002-alt")?.intId).toBeUndefined()
    expect(first.catalog.summary(first.printing.id)?.intId).toBe(1102)
    const rebuilt = createCatalog(first.snapshot)
    for (const card of rebuilt.index.cards)
      for (const row of rebuilt.index.printingsOf(stringValue(card["id"]))) {
        const id = stringValue(row["id"])
        expect(first.catalog.index.printing(id)).toEqual(row)
        expect(first.catalog.summary(id)).toEqual(rebuilt.summary(id))
      }
  })
  it("discards a stale ID hint when its URL names another printing", async () => {
    const h = client()
    const previous = await locateCard(h.client, { namespace: "official", key: "BP01-001" })
    const result = await locateCard(h.client, {
      namespace: "official",
      key: "BP01-002a",
      hint: previous.printing,
    })
    expect(result.printing.id).toBe("p:bp01-002-alt")
    expect(result.catalog.summary(result.printing.id)?.intId).toBe(1102)
  })
  it("uses a provisional integer and rejects its label on official routes with or without a hint", async () => {
    const h = provisionalClient()
    const result = await locateCard(h.client, { namespace: "provisional", key: "1102" })
    expect(result.printing).toMatchObject({
      id: "p:bp01-002-alt",
      cardNoState: "provisional",
      intId: 1102,
    })
    expect(result.catalog.summary(result.printing.id)?.cardNoState).toBe("provisional")
    const hinted = await locateCard(h.client, {
      namespace: "provisional",
      key: "1102",
      hint: result.printing,
    })
    expect(hinted.printing).toEqual(result.printing)
    await expect(
      locateCard(h.client, { namespace: "official", key: "BP01-002a" }),
    ).rejects.toMatchObject({ kind: "not-found" })
    await expect(
      locateCard(h.client, {
        namespace: "official",
        key: "BP01-002a",
        hint: result.printing,
      }),
    ).rejects.toMatchObject({ kind: "not-found" })
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
