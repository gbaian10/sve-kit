// @vitest-environment node
import { readFileSync } from "node:fs"
import { brotliCompressSync } from "node:zlib"

import { parse } from "jsonc-parser"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import worker from "../src/cloudflare/worker"

type Env = Parameters<typeof worker.fetch>[1]
type Context = Parameters<typeof worker.fetch>[2]
type ReadObject = NonNullable<Awaited<ReturnType<Env["PREVIEW_BUCKET"]["get"]>>>
const HASH = "a".repeat(64)
const BLOB = `snapshots/blobs/${HASH}.json`
const INDEX = "snapshots/versions/index.json"
const IMMUTABLE = "private, max-age=31536000, immutable"

function fixture(bytes: Uint8Array = new TextEncoder().encode('{"synthetic":true}')) {
  const cancel = vi.fn()
  const metadata: NonNullable<ReadObject["httpMetadata"]> = {
    contentType: "application/json; charset=utf-8",
    cacheControl: "public, max-age=31536000, immutable",
  }
  const object: ReadObject = {
    httpEtag: '"synthetic-etag"',
    httpMetadata: metadata,
    body: new ReadableStream({
      start(controller) {
        controller.enqueue(bytes)
        controller.close()
      },
      cancel,
    }),
  }
  const get = vi.fn<Env["PREVIEW_BUCKET"]["get"]>().mockResolvedValue(object)
  const head = vi.fn<Env["PREVIEW_BUCKET"]["head"]>().mockResolvedValue({
    httpEtag: object.httpEtag,
    httpMetadata: metadata,
  })
  const assets = vi
    .fn<Env["ASSETS"]["fetch"]>()
    .mockImplementation(() =>
      Promise.resolve(
        new Response("<html>Temporary page</html>", { headers: { "Content-Type": "text/html" } }),
      ),
    )
  const env: Env = { PREVIEW_BUCKET: { get, head }, ASSETS: { fetch: assets } }
  const pending: Promise<unknown>[] = []
  const ctx: Context = { waitUntil: (promise) => pending.push(promise) }
  const flush = () => Promise.all(pending)
  return { env, ctx, flush, object, metadata, get, head, assets, cancel }
}

function request(path: string, init?: RequestInit): Request {
  const url = `https://dev.svekit.app${path}`
  const result = new Request(url, init)
  // Retain adversarial spellings that the test runtime's Request constructor would normalize away.
  Object.defineProperty(result, "url", { value: url })
  return result
}

const entries = new Map<string, { bytes: ArrayBuffer; headers: Headers }>()
const edgeCache = {
  match: vi.fn((key: Request) => {
    const entry = entries.get(key.url)
    return Promise.resolve(
      entry === undefined
        ? undefined
        : new Response(entry.bytes.slice(0), { headers: entry.headers }),
    )
  }),
  put: vi.fn(async (key: Request, response: Response) => {
    if (
      key.method !== "GET" ||
      response.status !== 200 ||
      !response.headers.get("Cache-Control")?.startsWith("public,")
    ) {
      throw new Error("Uncacheable response")
    }
    entries.set(key.url, {
      bytes: await response.arrayBuffer(),
      headers: new Headers(response.headers),
    })
  }),
}

beforeEach(() => {
  entries.clear()
  edgeCache.match.mockClear()
  edgeCache.put.mockClear()
  vi.stubGlobal("caches", { default: edgeCache })
})

afterEach(() => {
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

describe("development Worker", () => {
  it.each(["GET", "HEAD"])("refuses the publication inventory via %s", async (method) => {
    const f = fixture()
    const response = await worker.fetch(
      request("/cdn-preview/_publish/images.json", { method }),
      f.env,
      f.ctx,
    )
    expect(response.status).toBe(404)
    expect(f.get).not.toHaveBeenCalled()
    expect(f.head).not.toHaveBeenCalled()
    expect(f.assets).not.toHaveBeenCalled()
    expect(edgeCache.match).not.toHaveBeenCalled()
  })

  it.each([
    BLOB,
    `${BLOB}.br`,
    `snapshots/manifests/${HASH}.json`,
    `snapshots/manifests/${HASH}.json.br`,
  ])("serves immutable %s from the edge after an R2 miss", async (key) => {
    const f = fixture()
    const first = await worker.fetch(
      request(`/cdn-preview/${key}?v=1`, { headers: { Cookie: "CF_Authorization=first" } }),
      f.env,
      f.ctx,
    )
    expect(first.status).toBe(200)
    expect(first.headers.get("Cache-Control")).toBe(IMMUTABLE)
    expect(await first.text()).toBe('{"synthetic":true}')
    expect(f.get).toHaveBeenCalledExactlyOnceWith(key)
    await f.flush()

    f.get.mockRejectedValue(new Error("R2 must not be needed for a hit"))
    const hit = await worker.fetch(
      request(`/cdn-preview/${key}?v=2`, {
        headers: { Cookie: "CF_Authorization=second", Range: "bytes=0-2" },
      }),
      f.env,
      f.ctx,
    )
    expect(hit.status).toBe(200)
    expect(await hit.text()).toBe('{"synthetic":true}')
    expect(hit.headers.get("Cache-Control")).toBe(IMMUTABLE)
    expect(hit.headers.get("Content-Type")).toBe("application/json; charset=utf-8")
    expect(hit.headers.get("ETag")).toBe('"synthetic-etag"')
    expect(hit.headers.get("X-Sve-Browser-Cache-Control")).toBeNull()
    expect(f.get).toHaveBeenCalledTimes(1)
    for (const [cacheRequest] of edgeCache.match.mock.calls) {
      expect(cacheRequest.url).toBe(`https://dev.svekit.app/cdn-preview/${key}`)
      expect(cacheRequest.method).toBe("GET")
      expect([...cacheRequest.headers]).toEqual([])
    }
  })

  it.each([INDEX, "images/card_m/20001.webp", "images/art_s/20001-f1.webp"])(
    "reads updated %s from R2 even with a version query",
    async (key) => {
      const f = fixture()
      f.get
        .mockResolvedValueOnce(f.object)
        .mockResolvedValueOnce(fixture(new TextEncoder().encode("updated")).object)
      const first = await worker.fetch(request(`/cdn-preview/${key}?v=1`), f.env, f.ctx)
      expect(await first.text()).toBe('{"synthetic":true}')
      await f.flush()
      const second = await worker.fetch(request(`/cdn-preview/${key}?v=1`), f.env, f.ctx)
      expect(await second.text()).toBe("updated")
      expect(f.get).toHaveBeenCalledTimes(2)
      expect(edgeCache.match).not.toHaveBeenCalled()
      expect(edgeCache.put).not.toHaveBeenCalled()
    },
  )

  it.each(["GET", "HEAD"])(
    "preserves conditions and compressed representations on cache hits (%s)",
    async (method) => {
      const json = '{"synthetic":true}'
      const variants = [
        { suffix: "", bytes: new TextEncoder().encode(json), encoding: undefined },
        { suffix: ".br", bytes: brotliCompressSync(json), encoding: "br" },
      ]
      for (const variant of variants) {
        const f = fixture(variant.bytes)
        if (variant.encoding !== undefined) f.metadata.contentEncoding = variant.encoding
        f.object.httpEtag = `"etag${variant.suffix}"`
        const first = await worker.fetch(
          request(`/cdn-preview/${BLOB}${variant.suffix}`),
          f.env,
          f.ctx,
        )
        await first.arrayBuffer()
        await f.flush()
      }

      const f = fixture()
      f.get.mockRejectedValue(new Error("unexpected R2 read"))
      f.head.mockRejectedValue(new Error("unexpected R2 metadata read"))
      const options = vi.fn()
      const OriginalResponse = Response
      vi.stubGlobal(
        "Response",
        class extends OriginalResponse {
          constructor(...args: ConstructorParameters<typeof OriginalResponse>) {
            super(...args)
            options(args[1])
          }
        },
      )
      for (const variant of variants) {
        for (const condition of [
          undefined,
          '"different"',
          `W/"etag${variant.suffix}"`,
          `"other", "etag${variant.suffix}"`,
          "*",
        ]) {
          const matches = condition !== undefined && condition !== '"different"'
          const response = await worker.fetch(
            request(`/cdn-preview/${BLOB}${variant.suffix}`, {
              method,
              headers: condition === undefined ? {} : { "If-None-Match": condition },
            }),
            f.env,
            f.ctx,
          )
          expect(response.status).toBe(matches ? 304 : 200)
          expect(response.headers.get("ETag")).toBe(`"etag${variant.suffix}"`)
          expect(response.headers.get("Content-Encoding")).toBe(variant.encoding ?? null)
          expect(response.headers.get("Cache-Control")).toBe(IMMUTABLE)
          if (matches || method === "HEAD") expect(response.body).toBeNull()
          else {
            expect(new Uint8Array(await response.arrayBuffer())).toEqual(
              new Uint8Array(variant.bytes),
            )
            expect(options).toHaveBeenLastCalledWith(
              expect.objectContaining({ encodeBody: "manual" }),
            )
          }
        }
      }
      expect(f.get).not.toHaveBeenCalled()
      expect(f.head).not.toHaveBeenCalled()
    },
  )

  it.each(["lookup", "write", "sync-write", "unavailable"])(
    "keeps serving R2 when the cache %s fails",
    async (operation) => {
      const f = fixture()
      if (operation === "lookup")
        edgeCache.match.mockRejectedValueOnce(new Error("cache unavailable"))
      if (operation === "write") edgeCache.put.mockRejectedValueOnce(new Error("cache unavailable"))
      if (operation === "sync-write") {
        edgeCache.put.mockImplementationOnce(() => {
          throw new Error("cache unavailable")
        })
      }
      if (operation === "unavailable") vi.stubGlobal("caches", undefined)
      const response = await worker.fetch(request(`/cdn-preview/${BLOB}`), f.env, f.ctx)
      expect(response.status).toBe(200)
      expect(await response.text()).toBe('{"synthetic":true}')
      expect(response.headers.get("Cache-Control")).toBe(IMMUTABLE)
      await expect(f.flush()).resolves.toBeDefined()
    },
  )

  it("returns the R2 response while waitUntil keeps the cache write pending", async () => {
    const f = fixture()
    const write = Promise.withResolvers<undefined>()
    edgeCache.put.mockImplementationOnce(() => write.promise)
    try {
      const response = await worker.fetch(request(`/cdn-preview/${BLOB}`), f.env, f.ctx)
      expect(await response.text()).toBe('{"synthetic":true}')
      const complete = vi.fn()
      const background = f.flush().then(complete)
      await Promise.resolve()
      expect(complete).not.toHaveBeenCalled()
      write.resolve(undefined)
      await background
      expect(complete).toHaveBeenCalledOnce()
    } finally {
      write.resolve(undefined)
    }
  })

  it.each(["POST", "PUT", "PATCH", "DELETE", "OPTIONS"])("rejects %s", async (method) => {
    const f = fixture()
    const response = await worker.fetch(request(`/cdn-preview/${INDEX}`, { method }), f.env, f.ctx)
    expect(response.status).toBe(405)
    expect(response.headers.get("Allow")).toBe("GET, HEAD")
    expect(response.headers.get("Cache-Control")).toBe("no-store")
    expect(await response.text()).toBe("")
    expect(f.get).not.toHaveBeenCalled()
    expect(f.head).not.toHaveBeenCalled()
    expect(f.assets).not.toHaveBeenCalled()
    expect(edgeCache.match).not.toHaveBeenCalled()
    expect(edgeCache.put).not.toHaveBeenCalled()
  })

  it.each([
    BLOB,
    `${BLOB}.br`,
    `snapshots/manifests/${HASH}.json`,
    `snapshots/manifests/${HASH}.json.br`,
    INDEX,
    "images/card_s/20001.webp",
    "images/card_m/20001-f1.webp",
    "images/card_l/20001.webp",
    "images/art_s/20001-f2.webp",
    "images/art_m/9007199254740991.webp",
  ])("reads only the public key %s", async (key) => {
    const f = fixture()
    const response = await worker.fetch(request(`/cdn-preview/${key}?v=3`), f.env, f.ctx)
    expect(response.status).toBe(200)
    expect(await response.text()).toBe('{"synthetic":true}')
    expect(f.get).toHaveBeenCalledExactlyOnceWith(key)
    expect(f.assets).not.toHaveBeenCalled()
  })

  it.each([
    "/cdn-preview",
    "/cdn-preview/",
    "/cdn-preview/coordination/snapshot-v2-writer.json",
    "/cdn-preview/private/inputs/example.json",
    "/cdn-preview/reports/example.json",
    "/cdn-preview/sources/example.webp",
    "/cdn-preview/snapshots/preview/current.json",
    "/cdn-preview/snapshots/versions/index.json.gz",
    `/cdn-preview/${BLOB}.gz`,
    `/cdn-preview/snapshots/manifests/${HASH}.json.gz`,
    "/cdn-preview/snapshots/versions/index.json/secret",
    "/cdn-preview/snapshots/blobs/private.json",
    `/cdn-preview/snapshots/manifests/${HASH}.json/private`,
    `/cdn-preview/snapshots/blobs/${HASH}.json.png`,
    "/cdn-preview/images/sha256/aa/example.webp",
    "/cdn-preview/images/card_x/20001.webp",
    "/cdn-preview/images/card_m/20001.png",
    "/cdn-preview/images/card_m/0.webp",
    "/cdn-preview/images/card_m/020001.webp",
    "/cdn-preview/images/card_m/20001-f0.webp",
    "/cdn-preview/images/card_m/20001-f01.webp",
    "/cdn-preview/images/card_m/9007199254740992.webp",
    "/cdn-preview/images/card_m/20001-f9007199254740992.webp",
    `/cdn-preview/snapshots/blobs/../blobs/${HASH}.json`,
    `/cdn-preview/snapshots/blobs/./${HASH}.json`,
    `/cdn-preview/snapshots/blobs//${HASH}.json`,
    `/cdn-preview/snapshots%2fblobs/${HASH}.json`,
    `/cdn-preview/snapshots%2Fblobs/${HASH}.json`,
    `/cdn-preview/snapshots%252fblobs/${HASH}.json`,
    `/cdn-preview/snapshots/blobs/%2e%2e/blobs/${HASH}.json`,
    `/cdn-preview/snapshots/blobs/%252e%252e/blobs/${HASH}.json`,
    `/cdn-preview/snapshots\\blobs/${HASH}.json`,
    `/cdn-preview/snapshots%5cblobs/${HASH}.json`,
    `/cdn-preview/snapshots/blobs/${HASH}.json%00`,
    `/cdn-preview/snapshots/blobs/${HASH}.json%`,
    `/cdn-preview/snapshots/blobs/${HASH}.json\n`,
    `/cdn-preview%2fsnapshots/blobs/${HASH}.json`,
    `/cdn-preview%252fsnapshots/blobs/${HASH}.json`,
    `/cdn-preview\\snapshots/blobs/${HASH}.json`,
  ])("rejects the path %s without reading R2 or serving HTML", async (path) => {
    const f = fixture()
    const response = await worker.fetch(request(path), f.env, f.ctx)
    expect(response.status).toBe(404)
    expect(response.headers.get("Cache-Control")).toBe("no-store")
    expect(response.headers.get("Content-Type")).toBeNull()
    expect(await response.text()).toBe("")
    expect(f.get).not.toHaveBeenCalled()
    expect(f.head).not.toHaveBeenCalled()
    expect(f.assets).not.toHaveBeenCalled()
    expect(edgeCache.match).not.toHaveBeenCalled()
    expect(edgeCache.put).not.toHaveBeenCalled()
  })

  it.each(["GET", "HEAD"])("returns an empty 404 for missing objects (%s)", async (method) => {
    const f = fixture()
    f.get.mockResolvedValue(null)
    f.head.mockResolvedValue(null)
    const response = await worker.fetch(request(`/cdn-preview/${BLOB}`, { method }), f.env, f.ctx)
    expect(response.status).toBe(404)
    expect(await response.text()).toBe("")
    expect(response.headers.get("Cache-Control")).toBe("no-store")
    expect(f.assets).not.toHaveBeenCalled()
    expect(edgeCache.put).not.toHaveBeenCalled()
  })

  it("preserves Brotli bytes and metadata with manual encoding", async () => {
    const encoding = "br"
    const bytes = brotliCompressSync('{"synthetic":true}')
    const f = fixture(bytes)
    f.object.httpMetadata = { contentType: "application/json", contentEncoding: encoding }
    const options = vi.fn()
    const OriginalResponse = Response
    vi.stubGlobal(
      "Response",
      class extends OriginalResponse {
        constructor(...args: ConstructorParameters<typeof OriginalResponse>) {
          super(...args)
          options(args[1])
        }
      },
    )
    const response = await worker.fetch(request(`/cdn-preview/${BLOB}.br`), f.env, f.ctx)
    expect(response.status).toBe(200)
    expect(new Uint8Array(await response.arrayBuffer())).toEqual(new Uint8Array(bytes))
    expect(response.headers.get("Content-Type")).toBe("application/json")
    expect(response.headers.get("Content-Encoding")).toBe(encoding)
    expect(response.headers.get("ETag")).toBe('"synthetic-etag"')
    expect(options).toHaveBeenCalledWith(expect.objectContaining({ encodeBody: "manual" }))
  })

  it("does not invent absent metadata", async () => {
    const f = fixture()
    delete f.object.httpMetadata
    const response = await worker.fetch(request(`/cdn-preview/${BLOB}`), f.env, f.ctx)
    expect(response.headers.get("Content-Type")).toBeNull()
    expect(response.headers.get("Content-Encoding")).toBeNull()
    expect(response.headers.get("ETag")).toBe('"synthetic-etag"')
    expect(response.headers.get("Cache-Control")).toBe("private, no-cache")
  })

  it.each(['"synthetic-etag"', 'W/"synthetic-etag"', '"other", W/"synthetic-etag"', " * "])(
    "returns 304 for If-None-Match %s",
    async (condition) => {
      const f = fixture()
      const response = await worker.fetch(
        request(`/cdn-preview/${BLOB}`, {
          headers: { "If-None-Match": condition },
        }),
        f.env,
        f.ctx,
      )
      expect(response.status).toBe(304)
      expect(response.body).toBeNull()
      expect(response.headers.get("ETag")).toBe('"synthetic-etag"')
      expect(response.headers.get("Cache-Control")).toBe(IMMUTABLE)
      expect(await f.object.body.getReader().read()).toEqual({ done: true, value: undefined })
      expect(edgeCache.put).not.toHaveBeenCalled()
    },
  )

  it("matches opaque ETags containing commas", async () => {
    const f = fixture()
    f.object.httpEtag = '"one,two"'
    const response = await worker.fetch(
      request(`/cdn-preview/${BLOB}`, {
        headers: { "If-None-Match": '"other", W/"one,two"' },
      }),
      f.env,
      f.ctx,
    )
    expect(response.status).toBe(304)
  })

  it.each(['"different"', "synthetic-etag", '"synthetic-etag"garbage'])(
    "returns the body for a nonmatching condition %s",
    async (condition) => {
      const f = fixture()
      const response = await worker.fetch(
        request(`/cdn-preview/${BLOB}`, {
          headers: { "If-None-Match": condition },
        }),
        f.env,
        f.ctx,
      )
      expect(response.status).toBe(200)
      expect(await response.text()).toBe('{"synthetic":true}')
    },
  )

  it.each([undefined, '"synthetic-etag"'])("HEAD uses only R2 metadata (%s)", async (condition) => {
    const f = fixture()
    const response = await worker.fetch(
      request(`/cdn-preview/${BLOB}`, {
        method: "HEAD",
        headers: condition === undefined ? {} : { "If-None-Match": condition },
      }),
      f.env,
      f.ctx,
    )
    expect(response.status).toBe(condition === undefined ? 200 : 304)
    expect(response.body).toBeNull()
    expect(response.headers.get("ETag")).toBe('"synthetic-etag"')
    expect(response.headers.get("Content-Type")).toBe("application/json; charset=utf-8")
    expect(response.headers.get("Cache-Control")).toBe(IMMUTABLE)
    expect(f.head).toHaveBeenCalledExactlyOnceWith(BLOB)
    expect(f.get).not.toHaveBeenCalled()
    expect(edgeCache.put).not.toHaveBeenCalled()
  })

  it.each([
    [BLOB, "public,max-age=31536000,immutable", "private,max-age=31536000,immutable"],
    [`snapshots/manifests/${HASH}.json.br`, "public, max-age=31536000, immutable", IMMUTABLE],
    [INDEX, "no-store", "no-store"],
    [
      "images/card_m/20001.webp?v=3",
      "public,max-age=86400,must-revalidate",
      "private,max-age=86400,must-revalidate",
    ],
    [
      "images/art_s/20001-f1.webp",
      "public,max-age=86400,must-revalidate",
      "private,max-age=86400,must-revalidate",
    ],
    [
      "images/card_m/20001.webp?v=invalid",
      "public,max-age=86400,must-revalidate",
      "private,max-age=86400,must-revalidate",
    ],
    [BLOB, "max-age=60, public, must-revalidate", "max-age=60, private, must-revalidate"],
    [BLOB, "PUBLIC , max-age=60", "private , max-age=60"],
    [BLOB, "private, no-cache", "private, no-cache"],
    [BLOB, 'max-age=60, x-public=1, note="public"', 'max-age=60, x-public=1, note="public"'],
    [BLOB, undefined, "private, no-cache"],
    ["images/card_m/20001.webp?v=3", undefined, "private, no-cache"],
    [INDEX, undefined, "private, no-cache"],
    [BLOB, "no-store", "no-store"],
    [INDEX, "public, max-age=60", "private, max-age=60"],
  ])("preserves the publisher cache policy for %s (%s)", async (key, cache, expected) => {
    const f = fixture()
    if (cache === undefined) delete f.metadata.cacheControl
    else f.metadata.cacheControl = cache
    const response = await worker.fetch(request(`/cdn-preview/${key}`), f.env, f.ctx)
    expect(response.status).toBe(200)
    expect(response.headers.get("Cache-Control")).toBe(expected)
    if (key.startsWith("snapshots/blobs/") || key.startsWith("snapshots/manifests/")) {
      await response.arrayBuffer()
      await f.flush()
      const hit = await worker.fetch(request(`/cdn-preview/${key}`), f.env, f.ctx)
      expect(hit.headers.get("Cache-Control")).toBe(expected)
      expect(hit.headers.get("X-Sve-Browser-Cache-Control")).toBeNull()
      expect(f.get).toHaveBeenCalledTimes(1)
    }
  })

  it.each(["HEAD", "GET"])(
    "preserves no-store for conditional index requests (%s)",
    async (method) => {
      const f = fixture()
      f.metadata.cacheControl = "no-store"
      const response = await worker.fetch(
        request(`/cdn-preview/${INDEX}`, {
          method,
          headers: { "If-None-Match": f.object.httpEtag },
        }),
        f.env,
        f.ctx,
      )
      expect(response.status).toBe(304)
      expect(response.headers.get("Cache-Control")).toBe("no-store")
    },
  )

  it("returns an empty uncached error when R2 fails", async () => {
    const f = fixture()
    f.get.mockRejectedValue(new Error("synthetic failure"))
    const response = await worker.fetch(request(`/cdn-preview/${BLOB}`), f.env, f.ctx)
    expect(response.status).toBe(502)
    expect(await response.text()).toBe("")
    expect(response.headers.get("Cache-Control")).toBe("no-store")
    expect(f.assets).not.toHaveBeenCalled()
    expect(edgeCache.put).not.toHaveBeenCalled()
  })

  it.each(["/", "/cards", "/decks", "/play", "/cdn-preview-info"])(
    "delegates %s to assets",
    async (path) => {
      const f = fixture()
      const req = request(path)
      const response = await worker.fetch(req, f.env, f.ctx)
      expect(response.headers.get("Content-Type")).toBe("text/html")
      expect(f.assets).toHaveBeenCalledExactlyOnceWith(req)
      expect(f.get).not.toHaveBeenCalled()
    },
  )

  it.each([200, 304])("marks hashed build output immutable (%i)", async (status) => {
    const f = fixture()
    f.assets.mockResolvedValue(
      new Response(status === 200 ? "export {}" : null, {
        status,
        headers: { "Content-Type": "text/javascript", "Cache-Control": "public, max-age=0" },
      }),
    )
    const response = await worker.fetch(request("/assets/index-B0oVQuOp.js"), f.env, f.ctx)
    expect(response.status).toBe(status)
    expect(response.headers.get("Cache-Control")).toBe("private, max-age=31536000, immutable")
    expect(response.headers.get("Content-Type")).toBe("text/javascript")
  })

  it.each(["/assets/index-B0oVQuOp.js", "/theme-boot.js", "/image-sw.js", "/assets/notes.js"])(
    "leaves %s revalidating when it is not a hashed file",
    async (path) => {
      const f = fixture()
      f.assets.mockResolvedValue(
        new Response(path === "/assets/index-B0oVQuOp.js" ? "<html></html>" : "x", {
          headers: {
            "Content-Type": path === "/assets/index-B0oVQuOp.js" ? "text/html" : "text/javascript",
            "Cache-Control": "public, max-age=0, must-revalidate",
          },
        }),
      )
      const response = await worker.fetch(request(path), f.env, f.ctx)
      expect(response.headers.get("Cache-Control")).toBe("public, max-age=0, must-revalidate")
    },
  )

  it("keeps the body and other headers of a pinned file", async () => {
    const f = fixture()
    f.assets.mockResolvedValue(
      new Response("body{}", {
        headers: {
          "Content-Type": "text/css",
          "Content-Encoding": "br",
          ETag: '"abc"',
          "Cache-Control": "public, max-age=0",
        },
      }),
    )
    const response = await worker.fetch(request("/assets/index-DZgCgIGp.css"), f.env, f.ctx)
    expect(await response.text()).toBe("body{}")
    expect(response.headers.get("Content-Encoding")).toBe("br")
    expect(response.headers.get("ETag")).toBe('"abc"')
    expect(response.headers.get("Cache-Control")).toBe("private, max-age=31536000, immutable")
  })

  it("passes a partial response through unchanged", async () => {
    const f = fixture()
    f.assets.mockResolvedValue(
      new Response("ab", {
        status: 206,
        headers: { "Content-Type": "font/woff2", "Cache-Control": "public, max-age=0" },
      }),
    )
    const response = await worker.fetch(
      request("/assets/ibm-plex-sans-latin-400-normal-CDDApCn2.woff2", {
        headers: { Range: "bytes=0-1" },
      }),
      f.env,
      f.ctx,
    )
    expect(response.status).toBe(206)
    expect(response.headers.get("Cache-Control")).toBe("public, max-age=0")
  })

  it("does not pin a missing hashed file", async () => {
    const f = fixture()
    f.assets.mockResolvedValue(new Response(null, { status: 404 }))
    const response = await worker.fetch(request("/assets/index-B0oVQuOp.js"), f.env, f.ctx)
    expect(response.status).toBe(404)
    expect(response.headers.get("Cache-Control")).toBeNull()
  })

  it.each([
    "https://old.workers.dev/",
    `https://old.workers.dev/cdn-preview/${INDEX}`,
    "http://dev.svekit.app/",
    "https://dev.svekit.app:8443/",
  ])("denies other origins on %s", async (url) => {
    const f = fixture()
    const response = await worker.fetch(new Request(url), f.env, f.ctx)
    expect(response.status).toBe(404)
    expect(f.get).not.toHaveBeenCalled()
    expect(f.assets).not.toHaveBeenCalled()
    expect(edgeCache.match).not.toHaveBeenCalled()
    expect(edgeCache.put).not.toHaveBeenCalled()
  })

  it("keeps all routes behind the Worker and disables unprotected platform URLs", () => {
    const config: unknown = parse(
      readFileSync(new URL("../wrangler.dev.jsonc", import.meta.url), "utf8"),
    )
    expect(config).toMatchObject({
      name: "svekit-web-dev",
      main: "src/cloudflare/worker.ts",
      workers_dev: false,
      preview_urls: false,
      routes: [{ pattern: "dev.svekit.app", custom_domain: true }],
      r2_buckets: [{ binding: "PREVIEW_BUCKET", bucket_name: "svekit-dev" }],
      assets: {
        directory: "./dist",
        binding: "ASSETS",
        not_found_handling: "single-page-application",
        run_worker_first: true,
      },
    })
    expect(config).not.toHaveProperty("account_id")
  })
})
