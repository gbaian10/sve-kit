import path from "node:path"

import { describe, expect, it } from "vitest"

import { snapshotRoots } from "../vite.config"

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

  it.each(["SVE_EXPORT_DIR", "SVE_PREVIEW_DIR"])("rejects empty or relative %s", (name) => {
    for (const value of ["", "relative"]) {
      expect(() => snapshotRoots({ [name]: value })).toThrow(
        `${name} must be a non-empty absolute path`,
      )
    }
  })
})
