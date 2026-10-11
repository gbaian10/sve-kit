import path from "node:path"

import { describe, expect, it } from "vitest"

import { snapshotCacheControl, snapshotEncoding, snapshotRoots } from "../vite.config"

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
    ["images/card_m/12.webp", "?v=3"],
  ])("caches content-addressed %s for good", (relative, search) => {
    expect(snapshotCacheControl(relative, search)).toBe("public, max-age=31536000, immutable")
  })

  it.each([
    ["snapshots/preview/current.json", ""],
    ["snapshots/versions/index.json", ""],
    ["images/card_m/12.webp", ""],
    ["images/card_m/12.webp", "?v=0"],
    ["snapshots/blobs/short.json", ""],
  ])("never caches %s%s", (relative, search) => {
    expect(snapshotCacheControl(relative, search)).toBe("no-store")
  })

  it("prefers Brotli, then gzip, then no encoding", () => {
    expect(snapshotEncoding("gzip, deflate, br, zstd")).toBe("br")
    expect(snapshotEncoding("gzip;q=1.0, identity")).toBe("gzip")
    expect(snapshotEncoding("identity")).toBeUndefined()
    expect(snapshotEncoding(undefined)).toBeUndefined()
  })
})
