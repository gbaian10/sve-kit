// @vitest-environment node
import { describe, expect, it } from "vitest"

import { buildSnapshot } from "../../scripts/fixture/build"
import { type Fetcher } from "./cdn"
import { createSnapshotClient, type SnapshotStatus } from "./client"
import { type JsonObject, parseStrict, stringValue } from "./format-v1/json"

const stubImage = ({ width, height, seed }: { width: number; height: number; seed: number }) =>
  Promise.resolve(
    new TextEncoder().encode(`stub-webp:${String(width)}x${String(height)}:${String(seed)}`),
  )
const built = await buildSnapshot({ encodeImage: stubImage })

interface Served {
  readonly fetcher: Fetcher
  readonly requests: string[]
  readonly files: Map<string, Uint8Array>
  fail: (predicate: (path: string) => boolean) => void
}

/** Serves an in-memory snapshot root under `base`, recording every request. */
function serve(base: string, files = new Map(built.files)): Served {
  const requests: string[] = []
  let failing: (path: string) => boolean = () => false
  const fetcher: Fetcher = (url) => {
    if (!url.startsWith(`${base}/`)) return Promise.resolve(new Response(null, { status: 404 }))
    const path = url.slice(base.length + 1)
    requests.push(path)
    if (failing(path)) return Promise.reject(new TypeError("Failed to fetch"))
    const bytes = files.get(path)
    if (!bytes) return Promise.resolve(new Response(null, { status: 404 }))
    return Promise.resolve(new Response(bytes.slice().buffer, { status: 200 }))
  }
  return {
    fetcher,
    requests,
    files,
    fail: (predicate) => {
      failing = predicate
    },
  }
}

describe("createSnapshotClient", () => {
  it("walks the version index to a compatible snapshot and loads config plus bootstrap", async () => {
    const served = serve("/cdn")
    const client = createSnapshotClient("/cdn", { fetch: served.fetcher })
    const seen: SnapshotStatus[] = []
    client.subscribe(() => seen.push(client.status()))
    expect(client.status()).toEqual({ state: "idle" })
    await client.load()
    expect(client.status()).toEqual({ state: "ready", dataVersion: "20260929T000000Z-0001" })
    expect(seen.map((s) => (s.state === "loading" ? s.phase : s.state))).toEqual([
      "index",
      "manifest",
      "bootstrap",
      "ready",
    ])
    const snapshot = client.snapshot()
    expect(snapshot?.bootstrap.some((fragment) => fragment.table === "card")).toBe(true)
    expect(snapshot?.config["search"]).toEqual({
      grammar_version: "synthetic-v1",
      normalizer_version: "synthetic-v1",
    })
    // The last page only holds a 2.0.0 entry, so both pages are read but no 2.0.0 manifest is.
    expect(
      served.requests.filter((path) => path.startsWith("snapshots/versions/pages/")),
    ).toHaveLength(2)
    expect(served.requests.some((path) => path.includes("0000000000000000"))).toBe(false)
    expect(
      served.requests.some((path) => path.includes("programs") || path.startsWith("images/")),
    ).toBe(false)
    await client.load()
    expect(served.requests.filter((path) => path === "snapshots/versions/index.json")).toHaveLength(
      1,
    )
  })

  it("reports incompatible when no page has a readable entry", async () => {
    const files = new Map(built.files)
    const index = parseStrict(files.get("snapshots/versions/index.json") ?? "") as JsonObject
    const pages = index["pages"] as JsonObject[]
    files.set(
      "snapshots/versions/index.json",
      new TextEncoder().encode(JSON.stringify({ ...index, pages: [pages[1]] })),
    )
    const client = createSnapshotClient("/cdn", { fetch: serve("/cdn", files).fetcher })
    await client.load()
    expect(client.status()).toMatchObject({ state: "error", kind: "incompatible" })
    expect(client.snapshot()).toBeNull()
  })

  it("retries a hash mismatch once, then reports corruption", async () => {
    const served = serve("/cdn")
    const manifestPath = built.manifestPath
    served.files.set(manifestPath, new TextEncoder().encode("{}"))
    const client = createSnapshotClient("/cdn", { fetch: served.fetcher })
    await client.load()
    expect(client.status()).toMatchObject({ state: "error", kind: "corrupt" })
    expect(served.requests.filter((path) => path === manifestPath)).toHaveLength(2)
  })

  it("reports a network failure and recovers on retry", async () => {
    const served = serve("/cdn")
    served.fail((path) => path.startsWith("snapshots/blobs/"))
    const client = createSnapshotClient("/cdn", { fetch: served.fetcher })
    await client.load()
    expect(client.status()).toMatchObject({ state: "error", kind: "network" })
    served.fail(() => false)
    await client.retry()
    expect(client.status()).toMatchObject({ state: "ready" })
  })

  it("rejects a version index whose revision went backwards", async () => {
    const served = serve("/cdn")
    const client = createSnapshotClient("/cdn", { fetch: served.fetcher })
    await client.load()
    const index = parseStrict(served.files.get("snapshots/versions/index.json") ?? "") as JsonObject
    served.files.set(
      "snapshots/versions/index.json",
      new TextEncoder().encode(JSON.stringify({ ...index, revision: 0 })),
    )
    await client.reload()
    // A reload failure keeps the active snapshot; the regression shows up as the update error.
    expect(client.status()).toMatchObject({
      state: "ready",
      updateError: { kind: "corrupt", detail: expect.stringContaining("backwards") as string },
    })
  })

  it("loads detail files on demand, joins them onto the bootstrap and keeps an LRU", async () => {
    const served = serve("/cdn")
    const client = createSnapshotClient("/cdn", { fetch: served.fetcher, detailCacheSize: 1 })
    await client.load()
    const bp01 = await client.fragments("text/bp01")
    const printing = bp01.find((fragment) => fragment.table === "printing")
    expect(printing?.rows[0]?.["id"]).toBeTypeOf("string")
    expect((printing?.rows[0]?.["faces"] as JsonObject[])[0]?.["printed_text_state"]).toBeTypeOf(
      "string",
    )
    const revision = bp01.find((fragment) => fragment.table === "face_revision")
    expect(revision?.rows[0]?.["effect_unit_id"]).toBeTypeOf("string")
    const before = served.requests.length
    await client.fragments("text/bp01")
    expect(served.requests.length).toBe(before)
    await client.fragments("text/global")
    await client.fragments("text/bp01")
    expect(served.requests.length).toBe(before + 2)
    await expect(client.fragments("programs")).rejects.toThrow("not a detail file")
    await expect(client.fragments("nope")).rejects.toThrow("unknown file")
  })

  it("keeps two roots apart", async () => {
    const served = serve("/cdn")
    const other = serve("/cdn-preview")
    const client = createSnapshotClient("/cdn", { fetch: served.fetcher })
    const preview = createSnapshotClient("/cdn-preview", { fetch: other.fetcher })
    await client.load()
    expect(client.status().state).toBe("ready")
    expect(preview.status()).toEqual({ state: "idle" })
    expect(other.requests).toHaveLength(0)
  })

  it("reads a preview root through its pointer file", async () => {
    const files = new Map(built.files)
    const manifestHash = `sha256:${built.manifestPath.slice("snapshots/manifests/".length, -".json".length)}`
    files.set(
      "snapshots/preview/current.json",
      new TextEncoder().encode(
        JSON.stringify({
          manifest_path: built.manifestPath,
          manifest_sha256: manifestHash,
          data_version: "preview-20260929T000000Z-0001",
        }),
      ),
    )
    const client = createSnapshotClient("/cdn-preview", {
      fetch: serve("/cdn-preview", files).fetcher,
      entry: "preview",
    })
    await client.load()
    expect(client.status()).toMatchObject({ state: "ready" })
  })
})

describe("createSnapshotClient: reload and transport edge cases", () => {
  it("keeps the active snapshot when a reload fails and reports the failure", async () => {
    const served = serve("/cdn")
    const client = createSnapshotClient("/cdn", { fetch: served.fetcher })
    await client.load()
    const before = client.snapshot()
    const index = parseStrict(served.files.get("snapshots/versions/index.json") ?? "") as JsonObject
    served.files.set(
      "snapshots/versions/index.json",
      new TextEncoder().encode(JSON.stringify({ ...index, index_format: 3 })),
    )
    await client.reload()
    expect(client.status()).toMatchObject({
      state: "ready",
      dataVersion: before?.dataVersion,
      updateError: { kind: "incompatible" },
    })
    expect(client.snapshot()).toBe(before)
    expect((await client.fragments("text/bp01")).length).toBeGreaterThan(0)
    served.fail((path) => path === "snapshots/versions/index.json")
    await client.reload()
    expect(client.status()).toMatchObject({ state: "ready", updateError: { kind: "network" } })
    served.fail(() => false)
    served.files.set(
      "snapshots/versions/index.json",
      new TextEncoder().encode(JSON.stringify(index)),
    )
    await client.reload()
    expect(client.status()).toEqual({ state: "ready", dataVersion: before?.dataVersion })
  })

  it("stays ready with an updating phase while a reload runs", async () => {
    const served = serve("/cdn")
    const client = createSnapshotClient("/cdn", { fetch: served.fetcher })
    await client.load()
    const seen: SnapshotStatus[] = []
    client.subscribe(() => seen.push(client.status()))
    await client.reload()
    expect(seen.every((status) => status.state === "ready")).toBe(true)
    expect(seen.map((status) => (status.state === "ready" ? status.updating : undefined))).toEqual([
      "index",
      "manifest",
      "bootstrap",
      undefined,
    ])
  })

  it("treats a broken index_format as corrupt, not as an update prompt", async () => {
    const files = new Map(built.files)
    const index = parseStrict(files.get("snapshots/versions/index.json") ?? "") as JsonObject
    const { index_format: _format, ...withoutFormat } = index
    files.set(
      "snapshots/versions/index.json",
      new TextEncoder().encode(JSON.stringify(withoutFormat)),
    )
    const client = createSnapshotClient("/cdn", { fetch: serve("/cdn", files).fetcher })
    await client.load()
    expect(client.status()).toMatchObject({ state: "error", kind: "corrupt" })
  })

  it("treats an unsupported index format as incompatible, not corrupt", async () => {
    const files = new Map(built.files)
    const index = parseStrict(files.get("snapshots/versions/index.json") ?? "") as JsonObject
    files.set(
      "snapshots/versions/index.json",
      new TextEncoder().encode(JSON.stringify({ ...index, index_format: 3 })),
    )
    const client = createSnapshotClient("/cdn", { fetch: serve("/cdn", files).fetcher })
    await client.load()
    expect(client.status()).toMatchObject({ state: "error", kind: "incompatible" })
  })

  it("classifies a body that fails to arrive as a network error", async () => {
    const fetcher: Fetcher = () =>
      Promise.resolve({
        ok: true,
        status: 200,
        arrayBuffer: () => Promise.reject(new TypeError("body stream aborted")),
      } as unknown as Response)
    const client = createSnapshotClient("/cdn", { fetch: fetcher })
    await client.load()
    expect(client.status()).toMatchObject({
      state: "error",
      kind: "network",
      detail: "body stream aborted",
    })
  })

  it("shares an in-flight detail request even when the LRU is full", async () => {
    const served = serve("/cdn")
    const client = createSnapshotClient("/cdn", { fetch: served.fetcher, detailCacheSize: 1 })
    await client.load()
    const first = client.fragments("text/bp01")
    const other = client.fragments("text/global")
    const again = client.fragments("text/bp01")
    expect(again).toBe(first)
    await Promise.all([first, other, again])
    const bp01Path = stringValue(client.snapshot()?.files.get("text/bp01")?.["path"])
    expect(served.requests.filter((path) => path === bp01Path)).toHaveLength(1)
  })
})
