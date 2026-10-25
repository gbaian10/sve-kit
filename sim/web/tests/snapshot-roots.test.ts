import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs"
import { createServer, type IncomingMessage, request, type ServerResponse } from "node:http"
import type { AddressInfo } from "node:net"
import { tmpdir } from "node:os"
import path from "node:path"
import { brotliDecompressSync } from "node:zlib"

import { describe, expect, it } from "vitest"

import {
  serveSnapshotRoot,
  snapshotCacheControl,
  snapshotEncoding,
  snapshotRoots,
} from "../vite.config"

describe("local snapshot roots", () => {
  it("uses the synthetic fixture when no export is configured", () => {
    expect(snapshotRoots({})).toEqual({
      cdn: path.resolve(import.meta.dirname, "../fixtures/snapshot"),
    })
  })

  it("keeps the public and preview roots independent", () => {
    expect(snapshotRoots({ SVE_EXPORT_DIR: "/export", SVE_PREVIEW_DIR: "/preview" })).toEqual({
      cdn: "/export",
      preview: "/preview",
    })
  })

  it("ignores the removed CDN variable", () => {
    expect(snapshotRoots({ SVE_CDN_DIR: "/unused" })).toEqual(snapshotRoots({}))
  })

  it.each(["SVE_EXPORT_DIR", "SVE_PREVIEW_DIR"])("treats empty %s as unset", (name) => {
    expect(snapshotRoots({ [name]: "" })).toEqual(snapshotRoots({}))
  })

  it("treats both empty roots as unset", () => {
    expect(snapshotRoots({ SVE_EXPORT_DIR: "", SVE_PREVIEW_DIR: "" })).toEqual(snapshotRoots({}))
  })

  it("keeps a configured export root when the preview root is empty", () => {
    expect(snapshotRoots({ SVE_EXPORT_DIR: "/export", SVE_PREVIEW_DIR: "" })).toEqual({
      cdn: "/export",
    })
  })

  it.each(["SVE_EXPORT_DIR", "SVE_PREVIEW_DIR"])("rejects relative %s", (name) => {
    expect(() => snapshotRoots({ [name]: "relative" })).toThrow(
      `${name} must be an absolute path when set`,
    )
  })
})

describe("dev snapshot responses", () => {
  const blob = `snapshots/blobs/${"a".repeat(64)}.json`
  const manifest = `snapshots/manifests/${"b".repeat(64)}.json`

  it.each([
    [blob, ""],
    [manifest, ""],
    ["images/card_l/12.webp", "?v=3"],
  ])("caches content-addressed %s for good", (relative, search) => {
    expect(snapshotCacheControl(relative, search)).toBe("public, max-age=31536000, immutable")
  })

  it.each([
    ["snapshots/preview/current.json", ""],
    ["snapshots/versions/index.json", ""],
    ["images/card_l/12.webp", ""],
    ["images/card_l/12.webp", "?v=0"],
    ["snapshots/blobs/short.json", ""],
  ])("never caches %s%s", (relative, search) => {
    expect(snapshotCacheControl(relative, search)).toBe("no-store")
  })

  it("uses Brotli when accepted, otherwise no encoding", () => {
    expect(snapshotEncoding("gzip, deflate, br, zstd")).toBe("br")
    expect(snapshotEncoding("gzip;q=1.0, identity")).toBeUndefined()
    expect(snapshotEncoding("identity")).toBeUndefined()
    expect(snapshotEncoding(undefined)).toBeUndefined()
  })

  it("skips an encoding the client refuses with q=0", () => {
    expect(snapshotEncoding("br;q=0, gzip")).toBeUndefined()
    expect(snapshotEncoding("br; q=0.0, gzip;q=0")).toBeUndefined()
    expect(snapshotEncoding("br;q=0.5")).toBe("br")
  })
})

type Handler = (req: IncomingMessage, res: ServerResponse, next: () => void) => void

// Runs the plugin's middleware behind a real HTTP server, so the headers are what a browser gets.
async function served(dir: string) {
  let handler: Handler | undefined
  const plugin = serveSnapshotRoot("/cdn", dir)
  const configure = plugin.configurePreviewServer as (server: unknown) => void
  configure({ middlewares: { use: (h: Handler) => (handler = h) } })
  const server = createServer((req, res) => {
    handler?.(req, res, () => {
      res.statusCode = 418
      res.end()
    })
  })
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve))
  const { port } = server.address() as AddressInfo
  const get = (pathname: string, acceptEncoding: string) =>
    new Promise<{ headers: IncomingMessage["headers"]; body: Buffer }>((resolve, reject) => {
      request({ port, path: pathname, headers: { "accept-encoding": acceptEncoding } }, (res) => {
        const chunks: Buffer[] = []
        res.on("data", (chunk: Buffer) => chunks.push(chunk))
        res.on("end", () => {
          resolve({ headers: res.headers, body: Buffer.concat(chunks) })
        })
      })
        .on("error", reject)
        .end()
    })
  return { get, close: () => server.close() }
}

describe("snapshot middleware", () => {
  const hash = "c".repeat(64)
  const dir = mkdtempSync(path.join(tmpdir(), "sve-snapshot-root-"))
  mkdirSync(path.join(dir, "snapshots/blobs"), { recursive: true })
  mkdirSync(path.join(dir, "images"), { recursive: true })
  const json = JSON.stringify({ rows: Array.from({ length: 50 }, (_, i) => ({ id: i })) })
  writeFileSync(path.join(dir, `snapshots/blobs/${hash}.json`), json)
  writeFileSync(path.join(dir, "images/a.webp"), "webp")

  it("sends compressed JSON with the headers a cache needs", async () => {
    const server = await served(dir)
    try {
      const br = await server.get(`/cdn/snapshots/blobs/${hash}.json`, "gzip, br")
      expect(br.headers["content-encoding"]).toBe("br")
      expect(br.headers["vary"]).toBe("Accept-Encoding")
      expect(br.headers["cache-control"]).toBe("public, max-age=31536000, immutable")
      expect(brotliDecompressSync(br.body).toString()).toBe(json)
      const raw = await server.get(`/cdn/snapshots/blobs/${hash}.json`, "br;q=0, gzip")
      expect(raw.headers["content-encoding"]).toBeUndefined()
      expect(raw.body.toString()).toBe(json)
      const plain = await server.get(`/cdn/snapshots/blobs/${hash}.json`, "identity")
      expect(plain.headers["content-encoding"]).toBeUndefined()
      expect(plain.headers["vary"]).toBe("Accept-Encoding")
      expect(plain.body.toString()).toBe(json)
      const image = await server.get("/cdn/images/a.webp", "br")
      expect(image.headers["content-encoding"]).toBeUndefined()
      expect(image.headers["vary"]).toBeUndefined()
    } finally {
      server.close()
    }
  })
})
