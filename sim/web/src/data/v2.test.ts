// @vitest-environment node
import { describe, expect, it } from "vitest"

import { memoryCache } from "../test-utils/cache"
import { type Fetcher } from "./cdn"
import { createSnapshotClient } from "./client"
import {
  canonical,
  canonicalText,
  type JsonObject,
  objectValue,
  stringValue,
} from "./format-v1/json"
import { validateMedia } from "./format-v1/media"
import { readSnapshot } from "./format-v1/reader"
import { validateConfig, validateDigitalLinks } from "./format-v1/semantics"
import { imageUrl, mediaImageSource } from "./image-url"
import { loadImagePage } from "./images"
import { v2Fixture, v2Version } from "./v2-fixture"

function origin(first = v2Version()) {
  const files = new Map(first.files)
  const requests: string[] = []
  let index: JsonObject = { index_format: 2, revision: 1, current: first.entry, previous: null }
  const fetcher: Fetcher = (url) => {
    const path = url.slice("https://cdn.test/".length)
    requests.push(path)
    const bytes = path === "snapshots/versions/index.json" ? canonical(index) : files.get(path)
    return Promise.resolve(
      bytes ? new Response(bytes.slice().buffer) : new Response(null, { status: 404 }),
    )
  }
  return {
    files,
    requests,
    fetcher,
    setIndex: (value: JsonObject) => {
      index = value
    },
  }
}
const front = { printingId: "p:a", faceId: "f:a" }

describe("2.0 shared contract and display projection", () => {
  it("joins the independent shared golden and oracle without reinterpreting 1.x tuples", () => {
    const manifest = objectValue(v2Fixture("manifest.json"))
    const payloads = new Map(
      (manifest["files"] as JsonObject[]).map((f) => [
        stringValue(f["key"]),
        canonical(v2Fixture(`payloads/${stringValue(f["sha256"]).slice(7)}.json`)),
      ]),
    )
    expect(canonicalText(readSnapshot(manifest, payloads))).toBe(
      canonicalText(v2Fixture("expected-logical.json")),
    )
  })
  it("uses exactly the Python shared URL vectors, including permanent f7 and safe integer boundary", () => {
    for (const raw of v2Fixture("image-url-cases.json") as JsonObject[]) {
      expect(
        imageUrl(
          "https://cdn.test",
          Number(raw["int_id"]),
          Number(raw["ordinal"]),
          stringValue(raw["size"]),
          Number(raw["version"]),
        ),
      ).toBe(`https://cdn.test/${stringValue(raw["url"])}`)
    }
  })
  it("uses the art token separately and emits only distinct width descriptors for tiny sources", () => {
    const row = (
      objectValue(v2Fixture("expected-logical.json"))["printing_image"] as JsonObject[]
    )[0]
    if (!row) throw new Error("missing media")
    expect(mediaImageSource("/cdn", 20001, 7, row, "art")?.src).toBe(
      "/cdn/images/art_m/20001-f7.webp?v=11",
    )
    const cropChanged = { ...row, art_version: 12 }
    expect(mediaImageSource("/cdn", 20001, 7, cropChanged, "card")?.src).toBe(
      mediaImageSource("/cdn", 20001, 7, row, "card")?.src,
    )
    expect(mediaImageSource("/cdn", 20001, 7, cropChanged, "art")?.src).toBe(
      "/cdn/images/art_m/20001-f7.webp?v=12",
    )
    const tiny = {
      ...row,
      variants: (row["variants"] as JsonObject[]).map((v) => ({ ...v, width: 1, height: 1 })),
    }
    const source = mediaImageSource("/cdn", 1, 0, tiny, "card")
    expect(source?.srcSet.split(", ")).toHaveLength(1)
    expect(source?.width).toBe(1)
    expect(source?.height).toBe(1)
    const landscape = {
      ...row,
      variants: (row["variants"] as JsonObject[]).map((v) => ({ ...v, width: 128, height: 64 })),
    }
    expect(mediaImageSource("/cdn", 1, 0, landscape, "card")?.height).toBe(64)
  })
  it.each([
    [0, 0, "card_m", 1],
    [1, -1, "card_m", 1],
    [1, 0, "card_m", 0],
    [1, 0, "unknown", 1],
    [1.5, 0, "card_m", 1],
    [1, 0, "card_m", 9007199254740992],
  ])("rejects invalid URL components %j", (id, ordinal, size, version) => {
    expect(() => imageUrl("https://cdn.test", id, ordinal, size, version)).toThrow(
      "image-variant-unapproved",
    )
  })
  it("fetches one home media file, never global source details, and preserves real width srcset", async () => {
    const served = origin()
    const client = createSnapshotClient("https://cdn.test", {
      fetch: served.fetcher,
      cacheStorage: memoryCache(),
    })
    await client.load()
    expect(client.status()).toMatchObject({ state: "ready" })
    const before = served.requests.length
    const page = await loadImagePage(client, [front])
    expect(served.requests.slice(before)).toHaveLength(1)
    expect(page.cardImage("p:a", "f:a")).toEqual({
      src: "https://cdn.test/images/card_l/1.webp?v=7",
      srcSet:
        "https://cdn.test/images/card_s/1.webp?v=7 128w, https://cdn.test/images/card_m/1.webp?v=7 320w, https://cdn.test/images/card_l/1.webp?v=7 459w",
      width: 459,
      height: 641,
    })
    expect(
      (await loadImagePage(client, [{ printingId: "p:b", faceId: "f:b" }])).cardImage("p:b", "f:b")
        ?.src,
    ).toBe("https://cdn.test/images/card_l/2-f1.webp?v=7")
    const global = [...(client.snapshot()?.files.values() ?? [])]
      .filter((f) =>
        (f["row_counts"] as JsonObject[]).some(
          (c) => c["table"] === "image_asset" || c["table"] === "image_variant",
        ),
      )
      .map((f) => stringValue(f["path"]))
    await client.prefetchImages()
    expect(served.requests.some((p) => global.includes(p))).toBe(false)
  })
  it("loads optional global source details only on explicit demand", async () => {
    const served = origin()
    const client = createSnapshotClient("https://cdn.test", {
      fetch: served.fetcher,
      cacheStorage: memoryCache(),
    })
    await client.load()
    await loadImagePage(client, [front])
    const file = [...(client.snapshot()?.files.values() ?? [])].find((f) =>
      (f["row_counts"] as JsonObject[]).some((c) => c["table"] === "image_asset"),
    )
    if (!file) throw new Error("missing details")
    expect(served.requests).not.toContain(file["path"])
    const details = await client.fragments(stringValue(file["key"]))
    expect(details.some((f) => f.table === "image_asset")).toBe(true)
    expect(served.requests).toContain(file["path"])
  })
  it("rejects extra source-detail dependencies even when all references and hashes are valid", async () => {
    const version = v2Version(1, undefined, (manifest) => {
      const files = manifest["files"] as JsonObject[]
      const media = files.find((f) =>
        (f["row_counts"] as JsonObject[]).some((c) => c["table"] === "printing_image"),
      )
      const other = files.find(
        (f) =>
          f["role"] === "bootstrap" &&
          !(media?.["dependencies"] as JsonObject[]).some((d) => d["key"] === f["key"]),
      )
      if (!media || !other) throw new Error("missing candidate")
      media["dependencies"] = [
        ...(media["dependencies"] as JsonObject[]),
        { key: other["key"] ?? null, sha256: other["sha256"] ?? null },
      ].sort((a, b) => stringValue(a["key"]).localeCompare(stringValue(b["key"])))
    })
    const served = origin(version)
    const client = createSnapshotClient("https://cdn.test", { fetch: served.fetcher })
    await client.load()
    expect(client.status()).toMatchObject({ state: "ready" })
    await expect(loadImagePage(client, [front])).rejects.toThrow("dependency-closure")
  })
  it("rejects an absent media face rather than exposing a guessed URL", async () => {
    const served = origin(
      v2Version(1, (table, row) => {
        if (table === "printing_image" && row[0] === "p:a") row[1] = "f:absent"
      }),
    )
    const client = createSnapshotClient("https://cdn.test", { fetch: served.fetcher })
    await client.load()
    await expect(loadImagePage(client, [front])).rejects.toThrow("printing-image-face")
  })
  it.each(["missing", "withdrawn"])(
    "clears an old ready image after adopting %s, without borrowing another printing",
    async (state) => {
      const first = v2Version()
      const served = origin(first)
      const client = createSnapshotClient("https://cdn.test", { fetch: served.fetcher })
      await client.load()
      expect((await loadImagePage(client, [front])).cardImage("p:a", "f:a")).toBeDefined()
      const second = v2Version(2, (table, row) => {
        if (table !== "printing_image" || row[0] !== "p:a") return
        row[3] = state === "withdrawn" ? "withdrawn" : "approved"
        row[4] = state === "missing" ? "missing" : "available"
        row[5] = state === "withdrawn" ? "Synthetic withdrawal" : null
        row[6] = null
        row[7] = null
        row[8] = []
      })
      for (const [key, bytes] of second.files) served.files.set(key, bytes)
      served.setIndex({
        index_format: 2,
        revision: 2,
        current: second.entry,
        previous: first.entry,
      })
      await client.reload()
      const page = await loadImagePage(client, [front])
      expect(page.cardImage("p:a", "f:a")).toBeUndefined()
      expect(
        page.asset("p:a", "f:a")?.[state === "withdrawn" ? "publication_state" : "availability"],
      ).toBe(state)
    },
  )
  it("uses new v for every src/srcset after snapshot adoption; same bytes returning still get a new token", async () => {
    const served = origin()
    const client = createSnapshotClient("https://cdn.test", { fetch: served.fetcher })
    await client.load()
    for (const token of [8, 9]) {
      const next = v2Version(token === 8 ? 2 : 3, (table, row) => {
        if (table === "printing_image") row[6] = token
      })
      for (const [key, bytes] of next.files) served.files.set(key, bytes)
      served.setIndex({ index_format: 2, revision: token, current: next.entry, previous: null })
      await client.reload()
      const source = (await loadImagePage(client, [front])).cardImage("p:a", "f:a")
      expect(source?.src.endsWith(`?v=${String(token)}`)).toBe(true)
      expect(source?.srcSet.split(", ").every((url) => url.includes(`?v=${String(token)} `))).toBe(
        true,
      )
    }
  })
  it("requires both configured digital games in 2.0, even when there are no same_name links", () => {
    const file = (objectValue(v2Fixture("manifest.json"))["files"] as JsonObject[]).find(
      (f) => f["role"] === "config",
    )
    if (!file) throw new Error("missing config")
    const config = objectValue(v2Fixture(`payloads/${stringValue(file["sha256"]).slice(7)}.json`))
    expect(() => {
      validateConfig(config, "2.0.0")
    }).not.toThrow()
    const endpoints = config["digital_endpoints"] as JsonObject[]
    expect(() => {
      validateConfig(
        {
          ...config,
          digital_endpoints: endpoints.map((e, i) => ({ ...e, game: i === 0 ? "svwb" : "sv1" })),
        },
        "2.0.0",
      )
    }).toThrow("config-url-template")
    for (const values of [
      [],
      endpoints.slice(0, 1),
      [...endpoints, endpoints[0] ?? null],
      [...endpoints].reverse(),
    ])
      expect(() => {
        validateConfig({ ...config, digital_endpoints: values }, "2.0.0")
      }).toThrow("config-url-template")
    expect(() => {
      validateConfig(
        {
          ...config,
          digital_endpoints: endpoints.map((e) => ({ ...e, refresh_policy: "frozen" })),
        },
        "2.0.0",
      )
    }).toThrow("config-url-template")
  })
  it("rejects invalid versions/variants and malformed same_name, including duplicate human relation", () => {
    const row = (
      objectValue(v2Fixture("expected-logical.json"))["printing_image"] as JsonObject[]
    )[0]
    if (!row) throw new Error("missing media")
    for (const value of [
      { ...row, card_version: null },
      { ...row, variants: [] },
      { ...row, publication_state: "withdrawn" },
      {
        ...row,
        variants: [
          ...(row["variants"] as JsonObject[]),
          ...(row["variants"] as JsonObject[]).slice(0, 1),
        ],
      },
    ])
      expect(() => {
        validateMedia(value)
      }).toThrow()
    const link = {
      id: "dl:synthetic",
      card_id: "c:a",
      face_id: null,
      digital_card_id: "dc:a",
      digital_phase: null,
      relation: "same_name",
      effect_similarity: null,
      review_level: "unreviewed",
    }
    expect(() => {
      validateDigitalLinks({ digital_link: [link] })
    }).not.toThrow()
    for (const changed of [
      { face_id: "f:a" },
      { digital_phase: "base" },
      { effect_similarity: "same" },
      { review_level: "confirmed" },
    ])
      expect(() => {
        validateDigitalLinks({ digital_link: [{ ...link, ...changed }] })
      }).toThrow("schema")
    expect(() => {
      validateDigitalLinks({
        digital_link: [link, { ...link, id: "dl:human", relation: "same_card" }],
      })
    }).toThrow("primary-key-duplicate")
  })
})

describe("Index v2 finite window", () => {
  it.each(["text", "media"])(
    "aborts old %s requests at adoption and cannot install their late rows in the new snapshot",
    async (kind) => {
      const first = v2Version()
      const served = origin(first)
      const text = (first.manifest["files"] as JsonObject[]).find((f) =>
        kind === "text"
          ? f["role"] === "text"
          : (f["row_counts"] as JsonObject[]).some((c) => c["table"] === "printing_image"),
      )
      if (!text) throw new Error("missing text")
      let oldSignal: AbortSignal | null | undefined
      let finish: (() => void) | undefined
      let started: (() => void) | undefined
      const began = new Promise<void>((resolve) => {
        started = resolve
      })
      const fetcher: Fetcher = async (url, init) => {
        if (url.endsWith(stringValue(text["path"])) && !oldSignal) {
          oldSignal = init?.signal
          started?.()
          // An uncooperative transport finishes after replacement; it still cannot install old rows.
          await new Promise<void>((resolve) => {
            finish = resolve
          })
        }
        return served.fetcher(url, init)
      }
      const client = createSnapshotClient("https://cdn.test", { fetch: fetcher })
      await client.load()
      const old = client.fragments(stringValue(text["key"])).then(
        () => "unexpected",
        () => "cancelled",
      )
      await began
      const second = v2Version(2)
      for (const [path, bytes] of second.files) served.files.set(path, bytes)
      served.setIndex({
        index_format: 2,
        revision: 2,
        current: second.entry,
        previous: first.entry,
      })
      await client.reload()
      expect(oldSignal?.aborted).toBe(true)
      finish?.()
      expect(await old).toBe("cancelled")
      const rows = await client.fragments(stringValue(text["key"]))
      expect(rows.length).toBeGreaterThan(0)
      expect(client.snapshot()?.dataVersion).toBe(second.entry["data_version"])
    },
  )
  it("checks only previous when current is incompatible, and marks it older", async () => {
    const first = v2Version()
    const served = origin(first)
    served.setIndex({
      index_format: 2,
      revision: 2,
      current: { ...first.entry, data_version: "20261004T010002Z-0001", format_version: "9.0.0" },
      previous: first.entry,
    })
    const client = createSnapshotClient("https://cdn.test", { fetch: served.fetcher })
    await client.load()
    expect(client.status()).toMatchObject({ state: "ready", outdated: true })
    expect(client.snapshot()?.entrySource).toBe("previous")
    expect(served.requests.some((p) => p.includes("pages/"))).toBe(false)
  })
  it.each([{ required_capabilities: ["unknown-capability"] }, { min_reader_version: "9.0.0" }])(
    "uses previous for a future current entry %j without treating Index v2 as corrupt",
    async (change) => {
      const first = v2Version()
      const served = origin(first)
      served.setIndex({
        index_format: 2,
        revision: 2,
        current: { ...first.entry, data_version: "20261004T010002Z-0001", ...change },
        previous: first.entry,
      })
      const client = createSnapshotClient("https://cdn.test", { fetch: served.fetcher })
      await client.load()
      expect(client.status()).toMatchObject({ state: "ready", outdated: true })
    },
  )
  it("records a valid incompatible revision so a later index cannot silently roll back", async () => {
    const first = v2Version()
    const served = origin(first)
    const client = createSnapshotClient("https://cdn.test", { fetch: served.fetcher })
    await client.load()
    const active = client.snapshot()
    served.setIndex({
      index_format: 2,
      revision: 4,
      current: { ...first.entry, format_version: "9.0.0" },
      previous: null,
    })
    await client.reload()
    expect(client.status()).toMatchObject({ updateError: { kind: "incompatible" } })
    served.setIndex({ index_format: 2, revision: 3, current: first.entry, previous: null })
    await client.reload()
    expect(client.snapshot()).toBe(active)
    expect(client.status()).toMatchObject({ updateError: { kind: "corrupt" } })
  })
  it("keeps local active on incompatible current/previous, and prompts update without a local active", async () => {
    const served = origin()
    const client = createSnapshotClient("https://cdn.test", { fetch: served.fetcher })
    await client.load()
    const active = client.snapshot()
    const third = v2Version(3)
    served.setIndex({
      index_format: 2,
      revision: 3,
      current: { ...third.entry, format_version: "9.0.0" },
      previous: null,
    })
    await client.reload()
    expect(client.snapshot()).toBe(active)
    expect(client.status()).toMatchObject({
      state: "ready",
      outdated: true,
      updateError: { kind: "incompatible" },
    })
    const fresh = createSnapshotClient("https://cdn.test", { fetch: served.fetcher })
    await fresh.load()
    expect(fresh.status()).toMatchObject({ state: "error", kind: "incompatible" })
  })
  it("jumps from V1 to V3 using current, without traversing V2 changes or past entries", async () => {
    const served = origin()
    const client = createSnapshotClient("https://cdn.test", { fetch: served.fetcher })
    await client.load()
    const third = v2Version(3)
    for (const [key, bytes] of third.files) served.files.set(key, bytes)
    const second = v2Version(2)
    served.setIndex({ index_format: 2, revision: 3, current: third.entry, previous: second.entry })
    await client.reload()
    expect(client.snapshot()?.dataVersion).toBe(third.manifest["data_version"])
    expect(served.requests).not.toContain(stringValue(second.entry["manifest_path"]))
  })
  it.each(["text", "media"])(
    "recovers a retired %s fragment by rechecking current rather than manufacturing old data",
    async (kind) => {
      const first = v2Version()
      const served = origin(first)
      const client = createSnapshotClient("https://cdn.test", { fetch: served.fetcher })
      await client.load()
      const text = [...(client.snapshot()?.files.values() ?? [])].find((f) =>
        kind === "text"
          ? f["role"] === "text"
          : (f["row_counts"] as JsonObject[]).some((c) => c["table"] === "printing_image"),
      )
      if (!text) throw new Error("missing text")
      served.files.delete(stringValue(text["path"]))
      const third = v2Version(3)
      for (const [key, bytes] of third.files) if (key !== text["path"]) served.files.set(key, bytes)
      served.setIndex({ index_format: 2, revision: 3, current: third.entry, previous: null })
      await expect(client.fragments(stringValue(text["key"]))).rejects.toThrow("HTTP 404")
      expect(client.snapshot()?.dataVersion).toBe(third.manifest["data_version"])
    },
  )
  it("rejects an entry/manifest mismatch, repeated revision mutation and unknown index_format", async () => {
    const first = v2Version()
    const served = origin(first)
    served.setIndex({
      index_format: 2,
      revision: 1,
      current: { ...first.entry, published_at: "2026-10-04T01:00:02Z" },
      previous: null,
    })
    const client = createSnapshotClient("https://cdn.test", { fetch: served.fetcher })
    await client.load()
    expect(client.status()).toMatchObject({ state: "error", kind: "corrupt" })
    served.setIndex({ index_format: 2, revision: 2, current: first.entry, previous: null })
    await client.retry()
    served.setIndex({ index_format: 2, revision: 2, current: first.entry, previous: first.entry })
    await client.reload()
    expect(client.status()).toMatchObject({ updateError: { kind: "corrupt" } })
    served.setIndex({ index_format: 3, revision: 3, current: first.entry, previous: null })
    await client.reload()
    expect(client.status()).toMatchObject({ updateError: { kind: "incompatible" } })
  })
})
